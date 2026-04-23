use super::{REMOTE_ROOT_PATH, error::SshError};
use zeroize::Zeroizing;

/// Parsed remote file specification extracted from a user-supplied SSH URL.
#[derive(Debug, Clone)]
pub struct RemoteSpec {
    pub host: String,
    /// SSH port, defaults to 22.
    pub port: u16,
    /// Username parsed from the URL, if present. `None` means the UI must prompt.
    pub user: Option<String>,
    /// Absolute remote path, or a `~/`-prefixed relative path.
    pub path: String,
    /// Password embedded in the URL (immediately moved to the session cache, never re-emitted).
    pub password_in_url: Option<Zeroizing<String>>,
}

/// Build a stable `ssh://` URL from a `RemoteSpec` for persistence and recents.
///
/// ### Description
/// The generated URL never includes a password. Home-relative paths (`~/...`)
/// are encoded as `/~/...` so they can be round-tripped back to `~/...` by
/// `parse_remote_url`.
///
/// ### Arguments
/// - `spec`: Remote file metadata to encode.
///
/// ### Returns
/// - `String`: Canonical URL that can be fed back into `parse_remote_url`.
pub fn format_remote_url(spec: &RemoteSpec) -> String {
    let host = format_host_for_url(&spec.host);
    let authority = match spec.user.as_deref() {
        Some(user) if !user.is_empty() => format!("{user}@{host}:{}", spec.port),
        _ => format!("{host}:{}", spec.port),
    };
    let path = format_path_for_url(&spec.path);
    format!("ssh://{authority}{path}")
}

/// Parse a user-supplied string into a `RemoteSpec`.
///
/// ### Description
/// Accepted formats:
/// - `ssh://[user[:pass]@]host[:port]/absolute/path`
/// - `sftp://[user[:pass]@]host[:port]/absolute/path` (alias for `ssh://`)
/// - `ssh://[user[:pass]@]host[:port]` (host only, defaults to `/`)
/// - `[user@]host:/absolute/path` scp-style, absolute path
/// - `[user@]host:relative/path` scp-style, resolved relative to remote `$HOME`
/// - `[user@]host` scp-style host-only shorthand, defaults to `/`
///
/// IPv6 addresses must use bracket notation: `[::1]` or `[::1]:2222`.
///
/// ### Arguments
/// - `input`: Raw string from the URL input field; leading/trailing whitespace is trimmed.
///
/// ### Returns
/// - `Ok(RemoteSpec)`: Successfully parsed specification.
/// - `Err(SshError::ParseError)`: The input could not be understood.
pub fn parse_remote_url(input: &str) -> Result<RemoteSpec, SshError> {
    let input = input.trim();
    if input.is_empty() {
        return Err(SshError::ParseError("URL must not be empty".to_string()));
    }

    if let Some(rest) = input
        .strip_prefix("ssh://")
        .or_else(|| input.strip_prefix("sftp://"))
    {
        parse_url_style(rest)
    } else {
        parse_scp_style(input)
    }
}

/// Parse the authority and path from the portion of a URL that follows the `ssh://` or `sftp://` scheme.
///
/// ### Arguments
/// - `rest`: Everything after the scheme prefix, e.g. `"user@host:22/path"`.
///
/// ### Returns
/// - `Ok(RemoteSpec)`: Successfully parsed specification.
/// - `Err(SshError::ParseError)`: Empty host or invalid port.
fn parse_url_style(rest: &str) -> Result<RemoteSpec, SshError> {
    let (authority, path) = if let Some(slash_idx) = rest.find('/') {
        (
            &rest[..slash_idx],
            normalize_url_style_path(&rest[slash_idx..]),
        )
    } else {
        (rest, REMOTE_ROOT_PATH.to_string())
    };
    if authority.is_empty() {
        return Err(SshError::ParseError("Host must not be empty".to_string()));
    }

    let (userinfo, hostport) = match authority.rfind('@') {
        Some(idx) => (Some(&authority[..idx]), &authority[idx + 1..]),
        None => (None, authority),
    };

    let (host, port) = parse_hostport(hostport)?;

    let (user, password_in_url) = match userinfo {
        None => (None, None),
        Some(ui) => match ui.find(':') {
            None => (Some(ui.to_string()), None),
            Some(idx) => {
                let u = ui[..idx].to_string();
                let p = Zeroizing::new(ui[idx + 1..].to_string());
                (Some(u), Some(p))
            }
        },
    };

    Ok(RemoteSpec {
        host,
        port,
        user,
        path,
        password_in_url,
    })
}

/// Convert a runtime path into a URL path segment.
///
/// ### Arguments
/// - `path`: Runtime remote path.
///
/// ### Returns
/// - `String`: URL-ready absolute path.
fn format_path_for_url(path: &str) -> String {
    if let Some(relative) = path.strip_prefix("~/") {
        format!("/~/{relative}")
    } else if path == "~" {
        "/~".to_string()
    } else if path.starts_with('/') {
        path.to_string()
    } else {
        format!("/{path}")
    }
}

/// Normalize URL-style path tokens back to runtime representation.
///
/// ### Arguments
/// - `path`: URL-style absolute path as parsed from an `ssh://` URL.
///
/// ### Returns
/// - `String`: Runtime path where `/~/...` is mapped back to `~/...`.
fn normalize_url_style_path(path: &str) -> String {
    if path == "/~" {
        "~".to_string()
    } else if let Some(relative) = path.strip_prefix("/~/") {
        format!("~/{relative}")
    } else {
        path.to_string()
    }
}

/// Format hosts in URL authority form (wrap IPv6 literals with brackets).
///
/// ### Arguments
/// - `host`: Raw host or address.
///
/// ### Returns
/// - `String`: URL authority-safe host string.
fn format_host_for_url(host: &str) -> String {
    if host.contains(':') && !host.starts_with('[') && !host.ends_with(']') {
        format!("[{host}]")
    } else {
        host.to_string()
    }
}

/// Parse a `[user@]host:path` scp-style string into a `RemoteSpec`.
///
/// ### Arguments
/// - `input`: Full input string (already confirmed to have no `ssh://` or `sftp://` prefix).
///
/// ### Returns
/// - `Ok(RemoteSpec)`: Successfully parsed specification; relative paths are prefixed with `~/`,
///   and host-only forms default to `/`.
/// - `Err(SshError::ParseError)`: Empty host or invalid port.
fn parse_scp_style(input: &str) -> Result<RemoteSpec, SshError> {
    let (authority, path) = if let Some(colon_idx) = find_scp_path_separator(input) {
        let authority = &input[..colon_idx];
        let raw_path = &input[colon_idx + 1..];
        if raw_path.is_empty() {
            (authority, REMOTE_ROOT_PATH.to_string())
        } else if raw_path.starts_with(REMOTE_ROOT_PATH) {
            (authority, raw_path.to_string())
        } else {
            (authority, format!("~/{raw_path}"))
        }
    } else {
        (input, REMOTE_ROOT_PATH.to_string())
    };

    let (user, host_str) = match authority.find('@') {
        Some(idx) => (Some(authority[..idx].to_string()), &authority[idx + 1..]),
        None => (None, authority),
    };

    let (host, port) = parse_hostport(host_str)?;

    Ok(RemoteSpec {
        host,
        port,
        user,
        path,
        password_in_url: None,
    })
}

/// Find the scp authority/path separator (`:`) outside bracketed IPv6 literals.
///
/// ### Arguments
/// - `input`: SCP-style input such as `user@host:/path` or `user@[::1]:/path`.
///
/// ### Returns
/// - `Some(usize)`: Byte index of the separator colon.
/// - `None`: No path separator was found outside IPv6 brackets.
fn find_scp_path_separator(input: &str) -> Option<usize> {
    let mut in_brackets = false;
    for (idx, ch) in input.char_indices() {
        match ch {
            '[' => in_brackets = true,
            ']' => in_brackets = false,
            ':' if !in_brackets => return Some(idx),
            _ => {}
        }
    }
    None
}

/// Split a `host[:port]` or `[ipv6_addr][:port]` string into a host and port pair.
///
/// ### Arguments
/// - `hostport`: Host-and-port substring, e.g. `"example.com:2222"` or `"[::1]:22"`.
///
/// ### Returns
/// - `Ok((String, u16))`: Parsed host and port; port defaults to 22 when absent.
/// - `Err(SshError::ParseError)`: Empty host, unclosed IPv6 bracket, or non-numeric port.
fn parse_hostport(hostport: &str) -> Result<(String, u16), SshError> {
    if hostport.is_empty() {
        return Err(SshError::ParseError("Host must not be empty".to_string()));
    }

    if hostport.starts_with('[') {
        let close = hostport
            .find(']')
            .ok_or_else(|| SshError::ParseError("Unclosed '[' in IPv6 address".to_string()))?;
        let host = hostport[1..close].to_string();
        if host.is_empty() {
            return Err(SshError::ParseError("Host must not be empty".to_string()));
        }
        let rest = &hostport[close + 1..];
        let port = if rest.is_empty() {
            22
        } else if let Some(p) = rest.strip_prefix(':') {
            if p.is_empty() {
                return Err(SshError::ParseError("Port must not be empty".to_string()));
            }
            p.parse::<u16>()
                .map_err(|_| SshError::ParseError(format!("Invalid port: {p}")))?
        } else {
            return Err(SshError::ParseError(format!(
                "Invalid IPv6 host suffix: {rest}"
            )));
        };
        return Ok((host, port));
    }

    match hostport.rfind(':') {
        None => Ok((hostport.to_string(), 22)),
        Some(idx) => {
            let host = &hostport[..idx];
            let port_str = &hostport[idx + 1..];
            if host.is_empty() {
                return Err(SshError::ParseError("Host must not be empty".to_string()));
            }
            let port = port_str
                .parse::<u16>()
                .map_err(|_| SshError::ParseError(format!("Invalid port: {port_str}")))?;
            Ok((host.to_string(), port))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ssh_url_basic() {
        let spec = parse_remote_url("ssh://user@example.com/etc/nginx.conf").unwrap();
        assert_eq!(spec.host, "example.com");
        assert_eq!(spec.port, 22);
        assert_eq!(spec.user.as_deref(), Some("user"));
        assert_eq!(spec.path, "/etc/nginx.conf");
        assert!(spec.password_in_url.is_none());
    }

    #[test]
    fn sftp_scheme_alias() {
        let spec = parse_remote_url("sftp://user@example.com/home/user/file.txt").unwrap();
        assert_eq!(spec.host, "example.com");
        assert_eq!(spec.port, 22);
        assert_eq!(spec.path, "/home/user/file.txt");
    }

    #[test]
    fn custom_port() {
        let spec = parse_remote_url("ssh://admin@host.example.com:2222/srv/config").unwrap();
        assert_eq!(spec.port, 2222);
        assert_eq!(spec.host, "host.example.com");
    }

    #[test]
    fn password_in_url() {
        let spec = parse_remote_url("ssh://alice:s3cr3t@host/path/to/file").unwrap();
        assert_eq!(spec.user.as_deref(), Some("alice"));
        let pw = spec.password_in_url.unwrap();
        assert_eq!(pw.as_str(), "s3cr3t");
    }

    #[test]
    fn no_user_in_url() {
        let spec = parse_remote_url("ssh://host.example.com/etc/hosts").unwrap();
        assert!(spec.user.is_none());
    }

    #[test]
    fn scp_style_absolute() {
        let spec = parse_remote_url("user@server:/etc/nginx.conf").unwrap();
        assert_eq!(spec.host, "server");
        assert_eq!(spec.port, 22);
        assert_eq!(spec.user.as_deref(), Some("user"));
        assert_eq!(spec.path, "/etc/nginx.conf");
    }

    #[test]
    fn scp_style_relative() {
        let spec = parse_remote_url("user@server:notes/todo.txt").unwrap();
        assert_eq!(spec.path, "~/notes/todo.txt");
    }

    #[test]
    fn scp_style_no_user() {
        let spec = parse_remote_url("server:/etc/hosts").unwrap();
        assert!(spec.user.is_none());
        assert_eq!(spec.host, "server");
        assert_eq!(spec.path, "/etc/hosts");
    }

    #[test]
    fn ipv6_with_port() {
        let spec = parse_remote_url("ssh://user@[::1]:2222/home/user/file").unwrap();
        assert_eq!(spec.host, "::1");
        assert_eq!(spec.port, 2222);
    }

    #[test]
    fn ipv6_default_port() {
        let spec = parse_remote_url("ssh://user@[::1]/home/user/file").unwrap();
        assert_eq!(spec.host, "::1");
        assert_eq!(spec.port, 22);
    }

    #[test]
    fn scp_style_bracket_ipv6_absolute_path() {
        let spec = parse_remote_url("user@[::1]:/etc/hosts").unwrap();
        assert_eq!(spec.user.as_deref(), Some("user"));
        assert_eq!(spec.host, "::1");
        assert_eq!(spec.port, 22);
        assert_eq!(spec.path, "/etc/hosts");
    }

    #[test]
    fn scp_style_bracket_ipv6_relative_path() {
        let spec = parse_remote_url("user@[::1]:notes/todo.txt").unwrap();
        assert_eq!(spec.user.as_deref(), Some("user"));
        assert_eq!(spec.host, "::1");
        assert_eq!(spec.path, "~/notes/todo.txt");
    }

    #[test]
    fn bracketed_ipv6_with_invalid_suffix_fails() {
        assert!(parse_remote_url("ssh://user@[::1]junk/path").is_err());
    }

    #[test]
    fn bracketed_ipv6_with_empty_port_fails() {
        assert!(parse_remote_url("ssh://user@[::1]:/path").is_err());
    }

    #[test]
    fn ssh_url_without_path_defaults_to_root() {
        let spec = parse_remote_url("ssh://user@host").unwrap();
        assert_eq!(spec.user.as_deref(), Some("user"));
        assert_eq!(spec.host, "host");
        assert_eq!(spec.path, REMOTE_ROOT_PATH);
    }

    #[test]
    fn root_path_is_allowed() {
        let spec = parse_remote_url("ssh://user@host/").unwrap();
        assert_eq!(spec.path, REMOTE_ROOT_PATH);
    }

    #[test]
    fn scp_host_only_defaults_to_root() {
        let spec = parse_remote_url("user@server").unwrap();
        assert_eq!(spec.user.as_deref(), Some("user"));
        assert_eq!(spec.host, "server");
        assert_eq!(spec.path, REMOTE_ROOT_PATH);
    }

    #[test]
    fn empty_input_fails() {
        assert!(parse_remote_url("").is_err());
    }

    #[test]
    fn invalid_port_fails() {
        assert!(parse_remote_url("ssh://user@host:notaport/path").is_err());
    }

    #[test]
    fn port_out_of_range_fails() {
        assert!(parse_remote_url("ssh://user@host:99999/path").is_err());
    }

    #[test]
    fn whitespace_trimmed() {
        let spec = parse_remote_url("  ssh://user@host/path  ").unwrap();
        assert_eq!(spec.host, "host");
    }

    #[test]
    fn format_remote_url_roundtrips_absolute_path() {
        let original = RemoteSpec {
            host: "example.com".to_string(),
            port: 2222,
            user: Some("alice".to_string()),
            path: "/var/log/syslog".to_string(),
            password_in_url: None,
        };
        let formatted = format_remote_url(&original);
        let parsed = parse_remote_url(&formatted).unwrap();

        assert_eq!(parsed.host, original.host);
        assert_eq!(parsed.port, original.port);
        assert_eq!(parsed.user, original.user);
        assert_eq!(parsed.path, original.path);
        assert!(parsed.password_in_url.is_none());
    }

    #[test]
    fn format_remote_url_roundtrips_home_relative_path() {
        let original = RemoteSpec {
            host: "::1".to_string(),
            port: 22,
            user: Some("bob".to_string()),
            path: "~/projects/fulgur/notes.md".to_string(),
            password_in_url: None,
        };
        let formatted = format_remote_url(&original);
        let parsed = parse_remote_url(&formatted).unwrap();

        assert_eq!(parsed.host, original.host);
        assert_eq!(parsed.port, original.port);
        assert_eq!(parsed.user, original.user);
        assert_eq!(parsed.path, original.path);
        assert!(parsed.password_in_url.is_none());
    }
}
