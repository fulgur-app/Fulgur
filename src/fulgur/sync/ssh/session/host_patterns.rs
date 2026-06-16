use super::paths::known_hosts_path;
use hmac::{Hmac, KeyInit, Mac};
use sha1::Sha1;
use ssh_key::known_hosts::{Entry as KnownHostEntry, HostPatterns, KnownHosts};
use std::path::Path;

/// Build an SSH host-key algorithm preference from existing `known_hosts` entries.
///
/// ### Arguments
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `Some(String)`: Comma-separated `MethodType::HostKey` preference string.
/// - `None`: No known key types were found or `known_hosts` could not be parsed.
pub(super) fn hostkey_method_preferences_from_known_hosts(host: &str, port: u16) -> Option<String> {
    let known_hosts_path = known_hosts_path().ok()?;
    let known_types = known_host_algorithms_for_target(host, port, &known_hosts_path)?;
    if known_types.is_empty() {
        return None;
    }

    let mut merged: Vec<String> = Vec::new();
    for known in known_types {
        push_unique_hostkey_alg(&mut merged, &known);
    }
    for alg in default_hostkey_algorithms() {
        push_unique_hostkey_alg(&mut merged, alg);
    }
    Some(merged.join(","))
}

/// Extract matching host-key algorithms from parsed `known_hosts` entries.
///
/// ### Arguments
/// - `host`: Hostname or IP used for lookup.
/// - `port`: SSH port used for lookup.
/// - `known_hosts_path`: Path to the `known_hosts` file.
///
/// ### Returns
/// - `Some(Vec<String>)`: Distinct algorithm names in first-seen order.
/// - `None`: The file could not be parsed.
fn known_host_algorithms_for_target(
    host: &str,
    port: u16,
    known_hosts_path: &Path,
) -> Option<Vec<String>> {
    let entries = KnownHosts::read_file(known_hosts_path).ok()?;
    let mut key_types: Vec<String> = Vec::new();
    for entry in entries {
        if known_host_entry_matches_target(&entry, host, port) {
            push_unique_hostkey_alg(&mut key_types, entry.public_key().algorithm().as_ref());
        }
    }
    Some(key_types)
}

/// Check whether a `known_hosts` entry matches a target host and port.
///
/// ### Arguments
/// - `entry`: Parsed `known_hosts` entry.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `true`: The entry host pattern matches the target.
/// - `false`: The entry does not apply to the target.
pub(super) fn known_host_entry_matches_target(
    entry: &KnownHostEntry,
    host: &str,
    port: u16,
) -> bool {
    known_host_patterns_match(entry.host_patterns(), host, port)
}

/// Evaluate known-host host-pattern matching for a target host and port.
///
/// ### Arguments
/// - `patterns`: Parsed host pattern set for one known-host entry.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `true`: Pattern set matches this target host.
/// - `false`: Pattern set does not match.
fn known_host_patterns_match(patterns: &HostPatterns, host: &str, port: u16) -> bool {
    let candidates = host_match_candidates(host, port);
    match patterns {
        HostPatterns::Patterns(patterns) => match_known_host_patterns(patterns, &candidates),
        HostPatterns::HashedName { salt, hash } => candidates
            .iter()
            .any(|candidate| hashed_host_matches(salt, hash, candidate)),
    }
}

/// Build candidate host strings used by OpenSSH known-host matching.
///
/// ### Arguments
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
///
/// ### Returns
/// - `Vec<String>`: Candidate values to test against one pattern entry.
fn host_match_candidates(host: &str, port: u16) -> Vec<String> {
    if port == 22 {
        vec![host.to_string(), format!("[{host}]:22")]
    } else {
        vec![format!("[{host}]:{port}")]
    }
}

/// Match known-host pattern list with OpenSSH negation semantics.
///
/// ### Arguments
/// - `patterns`: Comma-separated host patterns from one known-host entry.
/// - `candidates`: Candidate host representations for the current connection.
///
/// ### Returns
/// - `true`: At least one positive pattern matched and no negated match rejected it.
/// - `false`: No positive match, or a negated pattern matched.
fn match_known_host_patterns(patterns: &[String], candidates: &[String]) -> bool {
    let mut matched = false;
    for pattern in patterns {
        let (negated, token) = if let Some(stripped) = pattern.strip_prefix('!') {
            (true, stripped)
        } else {
            (false, pattern.as_str())
        };

        if candidates
            .iter()
            .any(|candidate| wildcard_pattern_matches(token, candidate))
        {
            if negated {
                return false;
            }
            matched = true;
        }
    }
    matched
}

/// Match a target string against a known-host wildcard pattern.
///
/// ### Arguments
/// - `pattern`: Wildcard pattern using `*` and `?`.
/// - `target`: Candidate host string.
///
/// ### Returns
/// - `true`: Pattern matches the target.
/// - `false`: Pattern does not match.
fn wildcard_pattern_matches(pattern: &str, target: &str) -> bool {
    let pattern_chars: Vec<char> = pattern.chars().collect();
    let target_chars: Vec<char> = target.chars().collect();
    let mut pattern_index = 0usize;
    let mut target_index = 0usize;
    let mut star_pattern_index = None;
    let mut star_target_index = 0usize;

    while target_index < target_chars.len() {
        if pattern_index < pattern_chars.len()
            && (pattern_chars[pattern_index] == '?'
                || pattern_chars[pattern_index] == target_chars[target_index])
        {
            pattern_index += 1;
            target_index += 1;
        } else if pattern_index < pattern_chars.len() && pattern_chars[pattern_index] == '*' {
            star_pattern_index = Some(pattern_index);
            pattern_index += 1;
            star_target_index = target_index;
        } else if let Some(star_idx) = star_pattern_index {
            pattern_index = star_idx + 1;
            star_target_index += 1;
            target_index = star_target_index;
        } else {
            return false;
        }
    }

    while pattern_index < pattern_chars.len() && pattern_chars[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern_chars.len()
}

/// Validate a hashed known-host entry against a candidate host string.
///
/// ### Arguments
/// - `salt`: HMAC salt from the known-hosts hashed entry.
/// - `expected_hash`: SHA-1 HMAC output from the known-hosts hashed entry.
/// - `candidate`: Candidate host representation to verify.
///
/// ### Returns
/// - `true`: Candidate matches the hashed entry.
/// - `false`: Candidate does not match.
fn hashed_host_matches(salt: &[u8], expected_hash: &[u8; 20], candidate: &str) -> bool {
    type HmacSha1 = Hmac<Sha1>;
    let Ok(mut mac) = HmacSha1::new_from_slice(salt) else {
        return false;
    };
    mac.update(candidate.as_bytes());
    mac.verify_slice(expected_hash).is_ok()
}

/// Append a host-key algorithm to a list if it is not already present.
///
/// ### Arguments
/// - `list`: Ordered host-key algorithm list to mutate.
/// - `alg`: Candidate algorithm name.
fn push_unique_hostkey_alg(list: &mut Vec<String>, alg: &str) {
    if !alg.is_empty() && !list.iter().any(|existing| existing == alg) {
        list.push(alg.to_string());
    }
}

/// Return default host-key algorithms in preferred order.
///
/// ### Returns
/// - `&'static [&'static str]`: Built-in fallback algorithms.
fn default_hostkey_algorithms() -> &'static [&'static str] {
    &[
        "ssh-ed25519",
        "ecdsa-sha2-nistp256",
        "ecdsa-sha2-nistp384",
        "ecdsa-sha2-nistp521",
        "rsa-sha2-512",
        "rsa-sha2-256",
        "ssh-rsa",
    ]
}

#[cfg(test)]
mod tests {
    use super::{
        default_hostkey_algorithms, known_host_algorithms_for_target, push_unique_hostkey_alg,
        wildcard_pattern_matches,
    };

    #[test]
    fn wildcard_pattern_matches_supports_globs() {
        assert!(wildcard_pattern_matches("*.example.com", "dev.example.com"));
        assert!(wildcard_pattern_matches("srv-??", "srv-01"));
        assert!(!wildcard_pattern_matches("srv-??", "srv-001"));
    }

    #[test]
    fn known_host_algorithms_for_target_deduplicates_preserving_order() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let known_hosts_path = dir.path().join("known_hosts");
        std::fs::write(
            &known_hosts_path,
            "\
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ
example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X",
        )
        .expect("failed to write known_hosts");

        let key_types =
            known_host_algorithms_for_target("example.com", 22, &known_hosts_path).unwrap();
        assert_eq!(key_types, vec!["ssh-ed25519".to_string()]);
    }

    #[test]
    fn push_unique_hostkey_alg_deduplicates() {
        let mut list = vec!["ssh-ed25519".to_string()];
        push_unique_hostkey_alg(&mut list, "ssh-ed25519");
        push_unique_hostkey_alg(&mut list, "rsa-sha2-256");
        assert_eq!(
            list,
            vec!["ssh-ed25519".to_string(), "rsa-sha2-256".to_string()]
        );
    }

    #[test]
    fn default_hostkey_algorithms_is_not_empty() {
        assert!(!default_hostkey_algorithms().is_empty());
    }
}
