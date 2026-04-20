use super::error::SshError;
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

/// Parse a user-supplied string into a `RemoteSpec`.
///
/// ### Description
/// Accepted formats:
/// - `ssh://[user[:pass]@]host[:port]/absolute/path`
/// - `sftp://[user[:pass]@]host[:port]/absolute/path` (alias for `ssh://`)
/// - `[user@]host:/absolute/path` scp-style, absolute path
/// - `[user@]host:relative/path` scp-style, resolved relative to remote `$HOME`
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
/// - `Err(SshError::ParseError)`: Missing path, root-only path, empty host, or invalid port.
fn parse_url_style(rest: &str) -> Result<RemoteSpec, SshError> {
    let slash_idx = rest.find('/').ok_or_else(|| {
        SshError::ParseError("URL must include a path (e.g. ssh://host/path/to/file)".to_string())
    })?;

    let authority = &rest[..slash_idx];
    let path = rest[slash_idx..].to_string();

    if path == "/" {
        return Err(SshError::ParseError(
            "Path must point to a file, not just '/'".to_string(),
        ));
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

/// Parse a `[user@]host:path` scp-style string into a `RemoteSpec`.
///
/// ### Arguments
/// - `input`: Full input string (already confirmed to have no `ssh://` or `sftp://` prefix).
///
/// ### Returns
/// - `Ok(RemoteSpec)`: Successfully parsed specification; relative paths are prefixed with `~/`.
/// - `Err(SshError::ParseError)`: No colon separator, empty path, empty host, or invalid port.
fn parse_scp_style(input: &str) -> Result<RemoteSpec, SshError> {
    let colon_idx = input.find(':').ok_or_else(|| {
        SshError::ParseError(
            "Unrecognised format. Use ssh://user@host/path or user@host:/path".to_string(),
        )
    })?;

    let authority = &input[..colon_idx];
    let raw_path = &input[colon_idx + 1..];

    if raw_path.is_empty() {
        return Err(SshError::ParseError("Path must not be empty".to_string()));
    }

    // Prefix relative paths with ~/ so the remote shell resolves them against $HOME.
    let path = if raw_path.starts_with('/') {
        raw_path.to_string()
    } else {
        format!("~/{raw_path}")
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
        let rest = &hostport[close + 1..];
        let port = if let Some(p) = rest.strip_prefix(':') {
            p.parse::<u16>()
                .map_err(|_| SshError::ParseError(format!("Invalid port: {p}")))?
        } else {
            22
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
    fn missing_path_fails() {
        assert!(parse_remote_url("ssh://user@host").is_err());
    }

    #[test]
    fn root_path_only_fails() {
        assert!(parse_remote_url("ssh://user@host/").is_err());
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
}
