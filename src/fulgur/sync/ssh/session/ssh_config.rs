use super::host_patterns::match_known_host_patterns;
use super::identities::expand_tilde;
use super::paths::home_dir;
use std::path::{Path, PathBuf};

/// Port assumed by `RemoteSpec` when the URL does not name one.
const DEFAULT_SSH_PORT: u16 = 22;

/// Settings from `~/.ssh/config` that apply to one host alias.
///
/// Follows OpenSSH precedence: the first value obtained for a single-valued option wins,
/// while `IdentityFile` values accumulate. `Match` blocks are treated as non-matching
/// because their criteria are not evaluated, and `Include` is not followed.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SshHostConfig {
    /// Real hostname to connect to (`HostName`), with `%h` already expanded.
    pub host_name: Option<String>,
    /// Remote username (`User`).
    pub user: Option<String>,
    /// SSH port (`Port`).
    pub port: Option<u16>,
    /// Only offer the configured identities, even when the agent holds others.
    pub identities_only: bool,
    /// Raw `IdentityFile` values, expanded by `identity_files` once the user is known.
    identity_file_values: Vec<String>,
}

impl SshHostConfig {
    /// Load the settings that apply to a host alias from `~/.ssh/config`.
    ///
    /// ### Arguments
    /// - `alias`: Host as typed in the remote URL.
    ///
    /// ### Returns
    /// - `SshHostConfig`: Matching settings; empty when the file is missing or unreadable.
    #[must_use]
    pub fn load(alias: &str) -> Self {
        let Ok(home) = home_dir() else {
            return Self::default();
        };
        match std::fs::read_to_string(home.join(".ssh").join("config")) {
            Ok(config) => Self::parse(&config, alias),
            Err(_) => Self::default(),
        }
    }

    /// Extract the settings that apply to a host alias from `ssh_config` text.
    ///
    /// ### Arguments
    /// - `config`: Contents of an `ssh_config` file.
    /// - `alias`: Host matched against `Host` patterns.
    ///
    /// ### Returns
    /// - `SshHostConfig`: Settings in effect for the alias.
    fn parse(config: &str, alias: &str) -> Self {
        let alias_candidates = [alias.to_string()];
        let mut block_applies = true;
        let mut host_config = Self::default();
        for line in config.lines() {
            let Some((keyword, argument)) = split_config_line(line) else {
                continue;
            };
            let keyword = keyword.to_ascii_lowercase();
            if keyword == "host" {
                let patterns: Vec<String> = argument
                    .split_whitespace()
                    .map(|pattern| unquote(pattern).to_string())
                    .collect();
                block_applies = match_known_host_patterns(&patterns, &alias_candidates);
                continue;
            }
            if keyword == "match" {
                block_applies = false;
                continue;
            }
            if !block_applies {
                continue;
            }
            let value = unquote(argument);
            match keyword.as_str() {
                "hostname" if host_config.host_name.is_none() => {
                    host_config.host_name = expand_tokens(value, &[('h', alias)]);
                }
                "user" if host_config.user.is_none() => {
                    host_config.user = Some(value.to_string());
                }
                "port" if host_config.port.is_none() => {
                    host_config.port = value.parse().ok();
                }
                "identitiesonly" => {
                    host_config.identities_only |= value.eq_ignore_ascii_case("yes");
                }
                "identityfile" => host_config.identity_file_values.push(value.to_string()),
                "include" => log::debug!("ssh_config Include directives are not followed"),
                _ => {}
            }
        }
        host_config
    }

    /// Resolve the address to connect to for a remote URL host and port.
    ///
    /// `RemoteSpec` cannot tell an explicit `:22` from a missing port, so the configured
    /// `Port` applies whenever the URL port is the default.
    ///
    /// ### Arguments
    /// - `alias`: Host as typed in the remote URL.
    /// - `url_port`: Port from the remote URL.
    ///
    /// ### Returns
    /// - `(String, u16)`: Hostname and port to open the TCP connection to.
    #[must_use]
    pub fn connection_target(&self, alias: &str, url_port: u16) -> (String, u16) {
        let host = self.host_name.clone().unwrap_or_else(|| alias.to_string());
        let port = if url_port == DEFAULT_SSH_PORT {
            self.port.unwrap_or(url_port)
        } else {
            url_port
        };
        (host, port)
    }

    /// Expand the configured `IdentityFile` values into paths.
    ///
    /// ### Arguments
    /// - `alias`: Host as typed in the remote URL, used for `%h` when no `HostName` is set.
    /// - `user`: Remote username, substituted for `%r`.
    /// - `home`: Home directory, substituted for `~` and `%d`.
    ///
    /// ### Returns
    /// - `Vec<PathBuf>`: Expanded paths in file order, possibly non-existent. Values with
    ///   unsupported tokens or relative paths are skipped.
    #[must_use]
    pub fn identity_files(&self, alias: &str, user: &str, home: &Path) -> Vec<PathBuf> {
        let host = self.host_name.as_deref().unwrap_or(alias);
        let home_text = home.to_string_lossy();
        self.identity_file_values
            .iter()
            .filter_map(|raw| {
                let expanded = expand_tokens(raw, &[('d', &home_text), ('h', host), ('r', user)]);
                let path = expanded.map(|expanded| expand_tilde(&expanded, home));
                let path = path.filter(|path| path.is_absolute());
                if path.is_none() {
                    log::debug!("Ignoring unsupported IdentityFile value '{raw}'");
                }
                path
            })
            .collect()
    }
}

/// Split an `ssh_config` line into its keyword and argument.
///
/// ### Arguments
/// - `line`: Raw configuration line.
///
/// ### Returns
/// - `Some((keyword, argument))`: The line holds a directive; `argument` is trimmed.
/// - `None`: The line is blank, a comment, or has no argument.
fn split_config_line(line: &str) -> Option<(&str, &str)> {
    let line = line.trim();
    if line.is_empty() || line.starts_with('#') {
        return None;
    }
    let keyword_end = line.find(|c: char| c.is_whitespace() || c == '=')?;
    let (keyword, rest) = line.split_at(keyword_end);
    let rest = rest.trim_start();
    let argument = rest.strip_prefix('=').unwrap_or(rest).trim();
    (!argument.is_empty()).then_some((keyword, argument))
}

/// Remove one pair of surrounding double quotes.
///
/// ### Arguments
/// - `value`: Possibly quoted value.
///
/// ### Returns
/// - `&str`: The value without its surrounding quotes.
fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(value)
}

/// Expand `%x` tokens in an `ssh_config` value.
///
/// ### Arguments
/// - `raw`: Value possibly containing tokens.
/// - `tokens`: Supported token letters and their replacements; `%%` is always supported.
///
/// ### Returns
/// - `Some(String)`: The expanded value.
/// - `None`: The value uses a token that is not in `tokens`.
fn expand_tokens(raw: &str, tokens: &[(char, &str)]) -> Option<String> {
    let mut expanded = String::with_capacity(raw.len());
    let mut chars = raw.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            expanded.push(c);
            continue;
        }
        match chars.next()? {
            '%' => expanded.push('%'),
            token => {
                let (_, replacement) = tokens.iter().find(|(letter, _)| *letter == token)?;
                expanded.push_str(replacement);
            }
        }
    }
    Some(expanded)
}

#[cfg(test)]
mod tests {
    use super::{SshHostConfig, expand_tokens, split_config_line};
    use std::path::PathBuf;

    fn home() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\Users\alice")
        } else {
            PathBuf::from("/home/alice")
        }
    }

    #[test]
    fn split_config_line_handles_spaces_equals_and_comments() {
        assert_eq!(
            split_config_line("  IdentityFile ~/.ssh/id_work "),
            Some(("IdentityFile", "~/.ssh/id_work"))
        );
        assert_eq!(
            split_config_line("IdentityFile=~/.ssh/id_work"),
            Some(("IdentityFile", "~/.ssh/id_work"))
        );
        assert_eq!(
            split_config_line("IdentityFile = ~/.ssh/id_work"),
            Some(("IdentityFile", "~/.ssh/id_work"))
        );
        assert_eq!(split_config_line("# IdentityFile ~/.ssh/id_work"), None);
        assert_eq!(split_config_line("   "), None);
    }

    #[test]
    fn expand_tokens_rejects_unknown_tokens() {
        assert_eq!(
            expand_tokens("%h.example.com 100%%", &[('h', "dev")]),
            Some("dev.example.com 100%".to_string())
        );
        assert_eq!(expand_tokens("~/.ssh/%C", &[('h', "dev")]), None);
    }

    #[test]
    fn parse_reads_alias_block_like_openssh() {
        let config = "\
Host fulgurant_server
    HostName 203.0.113.7
    User debian
    Port 2222
    IdentityFile ~/.ssh/id_ed25519
    IdentitiesOnly yes
Host *
    User fallback
    IdentityFile ~/.ssh/id_global
";
        let host_config = SshHostConfig::parse(config, "fulgurant_server");
        assert_eq!(host_config.host_name.as_deref(), Some("203.0.113.7"));
        assert_eq!(host_config.user.as_deref(), Some("debian"));
        assert_eq!(host_config.port, Some(2222));
        assert!(host_config.identities_only);
        assert_eq!(
            host_config.identity_files("fulgurant_server", "debian", &home()),
            vec![
                home().join(".ssh/id_ed25519"),
                home().join(".ssh/id_global")
            ]
        );

        let other = SshHostConfig::parse(config, "example.com");
        assert_eq!(other.host_name, None);
        assert_eq!(other.user.as_deref(), Some("fallback"));
        assert!(!other.identities_only);
    }

    #[test]
    fn parse_follows_negated_patterns_match_blocks_and_quotes() {
        let home = home();
        let config = "\
IdentityFile ~/.ssh/global
Host *.example.com !legacy.example.com
    IdentityFile ~/.ssh/example
Match user root
    IdentityFile ~/.ssh/root
Host \"quoted.example.org\"
    IdentityFile \"~/.ssh/quoted key\"
";
        assert_eq!(
            SshHostConfig::parse(config, "dev.example.com").identity_files(
                "dev.example.com",
                "alice",
                &home
            ),
            vec![home.join(".ssh/global"), home.join(".ssh/example")]
        );
        assert_eq!(
            SshHostConfig::parse(config, "legacy.example.com").identity_files(
                "legacy.example.com",
                "alice",
                &home
            ),
            vec![home.join(".ssh/global")]
        );
        assert_eq!(
            SshHostConfig::parse(config, "quoted.example.org").identity_files(
                "quoted.example.org",
                "alice",
                &home
            ),
            vec![home.join(".ssh/global"), home.join(".ssh/quoted key")]
        );
    }

    #[test]
    fn identity_files_expand_tokens_and_skip_unsupported_values() {
        let config = "\
Host dev
    HostName dev.example.com
    IdentityFile ~/.ssh/id_%h_%r
    IdentityFile %d/.ssh/100%%
    IdentityFile ~/.ssh/%C
    IdentityFile relative/key
";
        let home = home();
        assert_eq!(
            SshHostConfig::parse(config, "dev").identity_files("dev", "bob", &home),
            vec![
                home.join(".ssh/id_dev.example.com_bob"),
                PathBuf::from(format!("{}/.ssh/100%", home.display())),
            ]
        );
    }

    #[test]
    fn hostname_expands_alias_token() {
        let config = "Host dev\n    HostName %h.internal.example.com\n";
        assert_eq!(
            SshHostConfig::parse(config, "dev").host_name.as_deref(),
            Some("dev.internal.example.com")
        );
    }

    #[test]
    fn connection_target_applies_config_port_only_to_default_port() {
        let host_config = SshHostConfig {
            host_name: Some("203.0.113.7".to_string()),
            port: Some(2222),
            ..SshHostConfig::default()
        };
        assert_eq!(
            host_config.connection_target("alias", 22),
            ("203.0.113.7".to_string(), 2222)
        );
        assert_eq!(
            host_config.connection_target("alias", 2200),
            ("203.0.113.7".to_string(), 2200)
        );
        assert_eq!(
            SshHostConfig::default().connection_target("alias", 22),
            ("alias".to_string(), 22)
        );
    }
}
