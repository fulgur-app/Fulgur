use crate::fulgur::sync::ssh::session::host_patterns::known_host_entry_matches_target;
use ssh_key::{
    PublicKey,
    known_hosts::{Entry as KnownHostEntry, KnownHosts, Marker as KnownHostMarker},
};
use std::path::Path;

/// Resolve host-key check result with a pure-Rust `known_hosts` fallback.
///
/// ### Arguments
/// - `known_hosts`: Loaded known-hosts collection.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `key`: Raw server host key returned by libssh2.
///
/// ### Returns
/// - `ssh2::CheckResult`: Resolved check result after optional fallback refinement.
pub(super) fn resolve_known_host_check_result_with_known_hosts_fallback(
    known_hosts: &ssh2::KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
    known_hosts_path: &Path,
) -> ssh2::CheckResult {
    let primary = resolve_known_host_check_result(known_hosts, host, port, key);
    let fallback = check_known_hosts_with_parser(host, port, key, known_hosts_path);
    merge_known_host_check_results(primary, fallback)
}

/// Merge libssh2's host-key check with marker-aware parser results.
///
/// ### Arguments
/// - `primary`: Result returned by libssh2's `known_hosts` implementation.
/// - `fallback`: Marker-aware result returned by the pure-Rust parser, when available.
///
/// ### Returns
/// - `ssh2::CheckResult::Mismatch`: The key is revoked, mismatched, or is only a certificate
///   authority key.
/// - `ssh2::CheckResult::Match`: An unmarked parsed entry or the primary lookup trusts the key.
/// - `ssh2::CheckResult::NotFound`: Neither lookup found an applicable entry.
/// - `ssh2::CheckResult::Failure`: Both lookups failed to produce a decision.
fn merge_known_host_check_results(
    primary: ssh2::CheckResult,
    fallback: Option<ParsedKnownHostCheckResult>,
) -> ssh2::CheckResult {
    match fallback {
        Some(ParsedKnownHostCheckResult::Revoked | ParsedKnownHostCheckResult::Mismatch) => {
            ssh2::CheckResult::Mismatch
        }
        Some(ParsedKnownHostCheckResult::Match) => ssh2::CheckResult::Match,
        Some(ParsedKnownHostCheckResult::NotFound) => {
            if matches!(primary, ssh2::CheckResult::Failure) {
                ssh2::CheckResult::NotFound
            } else {
                primary
            }
        }
        None => primary,
    }
}

/// Marker-aware result from the pure-Rust `known_hosts` parser.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ParsedKnownHostCheckResult {
    Match,
    Mismatch,
    NotFound,
    Revoked,
}

/// Resolve host-key check result across host representations used by OpenSSH.
///
/// ### Arguments
/// - `known_hosts`: Loaded known-hosts collection.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `key`: Raw server host key returned by libssh2.
///
/// ### Returns
/// - `ssh2::CheckResult::Match`: Any representation matched.
/// - `ssh2::CheckResult::Mismatch`: No matches and at least one representation mismatched.
/// - `ssh2::CheckResult::NotFound`: No matches/mismatches and at least one representation was missing.
/// - `ssh2::CheckResult::Failure`: All checks failed unexpectedly.
fn resolve_known_host_check_result(
    known_hosts: &ssh2::KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
) -> ssh2::CheckResult {
    let bracket_host = format!("[{host}]:{port}");
    if port == 22 {
        aggregate_check_results([
            known_hosts.check_port(host, port, key),
            known_hosts.check(host, key),
            known_hosts.check(&bracket_host, key),
        ])
    } else {
        aggregate_check_results([
            known_hosts.check_port(host, port, key),
            known_hosts.check(&bracket_host, key),
        ])
    }
}

/// Parse `known_hosts` with `ssh-key` and compare keys with the server key.
///
/// ### Arguments
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `key`: Raw server host key returned by libssh2.
/// - `known_hosts_path`: Path to the `known_hosts` file.
///
/// ### Returns
/// - `Some(ParsedKnownHostCheckResult)`: Parsed result from known-host entries.
/// - `None`: The file could not be read or the server key format is unsupported.
fn check_known_hosts_with_parser(
    host: &str,
    port: u16,
    key: &[u8],
    known_hosts_path: &Path,
) -> Option<ParsedKnownHostCheckResult> {
    let contents = std::fs::read_to_string(known_hosts_path).ok()?;
    let entries = KnownHosts::new(&contents)
        .filter_map(Result::ok)
        .collect::<Vec<_>>();
    let server_key = PublicKey::from_bytes(key).ok()?;
    Some(resolve_known_host_check_result_from_entries(
        &entries,
        host,
        port,
        &server_key,
    ))
}

/// Resolve host-key check result from parsed `known_hosts` entries.
///
/// ### Arguments
/// - `entries`: Parsed `known_hosts` entries.
/// - `host`: Hostname or IP used for the SSH connection.
/// - `port`: SSH port used for the SSH connection.
/// - `server_key`: Server key parsed from libssh2 raw bytes.
///
/// ### Returns
/// - `ParsedKnownHostCheckResult::Revoked`: An applicable revocation matches the server key.
/// - `ParsedKnownHostCheckResult::Match`: An unmarked entry matches the server key.
/// - `ParsedKnownHostCheckResult::Mismatch`: Host entries exist, but none directly trust the key.
/// - `ParsedKnownHostCheckResult::NotFound`: No host entry matched.
fn resolve_known_host_check_result_from_entries(
    entries: &[KnownHostEntry],
    host: &str,
    port: u16,
    server_key: &PublicKey,
) -> ParsedKnownHostCheckResult {
    let mut saw_host_entry = false;
    let mut saw_unmarked_match = false;
    for entry in entries {
        if !known_host_entry_matches_target(entry, host, port) {
            continue;
        }

        saw_host_entry = true;
        if entry.public_key().key_data() == server_key.key_data() {
            match entry.marker() {
                Some(KnownHostMarker::Revoked) => {
                    return ParsedKnownHostCheckResult::Revoked;
                }
                Some(KnownHostMarker::CertAuthority) => {}
                None => saw_unmarked_match = true,
            }
        }
    }

    if saw_unmarked_match {
        ParsedKnownHostCheckResult::Match
    } else if saw_host_entry {
        ParsedKnownHostCheckResult::Mismatch
    } else {
        ParsedKnownHostCheckResult::NotFound
    }
}

/// Aggregate several `CheckResult` values into a single decision.
///
/// ### Arguments
/// - `results`: Check results from different host representations.
///
/// ### Returns
/// - `ssh2::CheckResult`: Combined result following the documented priority order.
fn aggregate_check_results(
    results: impl IntoIterator<Item = ssh2::CheckResult>,
) -> ssh2::CheckResult {
    let mut saw_mismatch = false;
    let mut saw_not_found = false;
    for result in results {
        match result {
            ssh2::CheckResult::Match => return ssh2::CheckResult::Match,
            ssh2::CheckResult::Mismatch => saw_mismatch = true,
            ssh2::CheckResult::NotFound => saw_not_found = true,
            ssh2::CheckResult::Failure => {}
        }
    }
    if saw_mismatch {
        ssh2::CheckResult::Mismatch
    } else if saw_not_found {
        ssh2::CheckResult::NotFound
    } else {
        ssh2::CheckResult::Failure
    }
}

#[cfg(test)]
mod tests {
    use super::super::append::{append_known_hosts_entry, known_hosts_entry_host};
    use super::super::test_fixtures::{
        SERVER_KEY_OPENSSH, parse_known_host_entries, server_key, server_key_bytes,
    };
    use super::{
        ParsedKnownHostCheckResult, aggregate_check_results, check_known_hosts_with_parser,
        merge_known_host_check_results, resolve_known_host_check_result_from_entries,
    };
    use ssh_key::PublicKey;

    #[test]
    fn aggregate_prefers_match_over_mismatch() {
        let result = aggregate_check_results([
            ssh2::CheckResult::Mismatch,
            ssh2::CheckResult::Match,
            ssh2::CheckResult::NotFound,
        ]);
        assert!(matches!(result, ssh2::CheckResult::Match));
    }

    #[test]
    fn aggregate_returns_mismatch_when_no_match() {
        let result = aggregate_check_results([
            ssh2::CheckResult::Failure,
            ssh2::CheckResult::Mismatch,
            ssh2::CheckResult::NotFound,
        ]);
        assert!(matches!(result, ssh2::CheckResult::Mismatch));
    }

    #[test]
    fn aggregate_returns_not_found_before_failure() {
        let result =
            aggregate_check_results([ssh2::CheckResult::Failure, ssh2::CheckResult::NotFound]);
        assert!(matches!(result, ssh2::CheckResult::NotFound));
    }

    #[test]
    fn parsed_revocation_overrides_a_libssh2_match() {
        let result = merge_known_host_check_results(
            ssh2::CheckResult::Match,
            Some(ParsedKnownHostCheckResult::Revoked),
        );

        assert!(matches!(result, ssh2::CheckResult::Mismatch));
    }

    #[test]
    fn parser_fallback_returns_match_for_plain_entry() {
        let entries = parse_known_host_entries(
            "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);
        assert_eq!(result, ParsedKnownHostCheckResult::Match);
    }

    #[test]
    fn parser_fallback_returns_mismatch_for_different_key() {
        let entries = parse_known_host_entries(
            "example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);
        assert_eq!(result, ParsedKnownHostCheckResult::Mismatch);
    }

    #[test]
    fn parser_fallback_matches_hashed_entry() {
        let entries = parse_known_host_entries(
            "|1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);
        assert_eq!(result, ParsedKnownHostCheckResult::Match);
    }

    #[test]
    fn parser_rejects_a_revoked_only_key() {
        let entries = parse_known_host_entries(&format!(
            "@revoked example.com {}",
            SERVER_KEY_OPENSSH.trim_end_matches(" server")
        ));

        let result = resolve_known_host_check_result_from_entries(
            &entries,
            "example.com",
            22,
            &server_key(),
        );

        assert_eq!(result, ParsedKnownHostCheckResult::Revoked);
    }

    #[test]
    fn parser_does_not_let_a_malformed_line_hide_a_revocation() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");
        let key = SERVER_KEY_OPENSSH.trim_end_matches(" server");
        std::fs::write(
            &path,
            format!("this is not a valid entry\n@revoked example.com {key}"),
        )
        .expect("failed to seed known_hosts");

        let result = check_known_hosts_with_parser("example.com", 22, &server_key_bytes(), &path);

        assert_eq!(result, Some(ParsedKnownHostCheckResult::Revoked));
    }

    #[test]
    fn parser_revocation_overrides_an_ordinary_entry_in_either_order() {
        let key = SERVER_KEY_OPENSSH.trim_end_matches(" server");
        for entries in [
            parse_known_host_entries(&format!("example.com {key}\n@revoked example.com {key}")),
            parse_known_host_entries(&format!("@revoked example.com {key}\nexample.com {key}")),
        ] {
            let result = resolve_known_host_check_result_from_entries(
                &entries,
                "example.com",
                22,
                &server_key(),
            );

            assert_eq!(result, ParsedKnownHostCheckResult::Revoked);
        }
    }

    #[test]
    fn parser_rejects_a_revoked_wildcard_key() {
        let entries = parse_known_host_entries(&format!(
            "@revoked *.example.com {}",
            SERVER_KEY_OPENSSH.trim_end_matches(" server")
        ));

        let result = resolve_known_host_check_result_from_entries(
            &entries,
            "dev.example.com",
            22,
            &server_key(),
        );

        assert_eq!(result, ParsedKnownHostCheckResult::Revoked);
    }

    #[test]
    fn parser_rejects_a_revoked_hashed_key() {
        let entries = parse_known_host_entries(
            "@revoked |1|O33ESRMWPVkMYIwJ1Uw+n877jTo=|nuuC5vEqXlEZ/8BXQR7m619W6Ak= ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF",
        );
        let server_key = PublicKey::from_openssh(
            "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF server",
        )
        .expect("failed to parse server key");

        let result =
            resolve_known_host_check_result_from_entries(&entries, "example.com", 22, &server_key);

        assert_eq!(result, ParsedKnownHostCheckResult::Revoked);
    }

    #[test]
    fn parser_does_not_trust_a_certificate_authority_as_a_raw_host_key() {
        let entries = parse_known_host_entries(&format!(
            "@cert-authority example.com {}",
            SERVER_KEY_OPENSSH.trim_end_matches(" server")
        ));

        let result = resolve_known_host_check_result_from_entries(
            &entries,
            "example.com",
            22,
            &server_key(),
        );

        assert_eq!(result, ParsedKnownHostCheckResult::Mismatch);
    }

    #[test]
    fn appended_entry_is_recognized_on_the_next_connection() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");
        let known_host = known_hosts_entry_host("example.com", 2222);

        append_known_hosts_entry(&path, &known_host, &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        let entries = parse_known_host_entries(&contents);
        let result = resolve_known_host_check_result_from_entries(
            &entries,
            "example.com",
            2222,
            &server_key(),
        );
        assert_eq!(result, ParsedKnownHostCheckResult::Match);
    }
}
