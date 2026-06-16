use super::super::error::SshError;
use super::host_patterns::known_host_entry_matches_target;
use super::paths::{ensure_ssh_dir, known_hosts_path, set_file_permissions_600};
use sha2::{Digest, Sha256};
use ssh_key::{
    PublicKey,
    known_hosts::{Entry as KnownHostEntry, KnownHosts},
};
use ssh2::Session;
use std::path::Path;

use super::HostKeyDecision;

/// Verify the server's host key against `~/.ssh/known_hosts`.
///
/// ### Arguments
/// - `session`: Active SSH session after handshake, used to retrieve the server's host key.
/// - `host`: Hostname string, used for `known_hosts` lookup and callback.
/// - `port`: SSH port, used for `known_hosts` lookup and callback.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the key is not found.
///
/// ### Returns
/// - `Ok(())`: Host key verified or accepted by the user.
/// - `Err(SshError::HostKeyMismatch)`: Key in `known_hosts` does not match the server.
/// - `Err(SshError::ConnectionFailed)`: Key rejected by the user or I/O error on `known_hosts`.
pub(super) fn check_host_key(
    session: &Session,
    host: &str,
    port: u16,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
) -> Result<(), SshError> {
    let kh_path = known_hosts_path()?;
    let mut known_hosts = session
        .known_hosts()
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    if kh_path.exists() {
        known_hosts
            .read_file(&kh_path, ssh2::KnownHostFileKind::OpenSSH)
            .map_err(|e| SshError::ConnectionFailed(format!("Failed to read known_hosts: {e}")))?;
    }

    let (key, key_type) = session
        .host_key()
        .ok_or_else(|| SshError::ConnectionFailed("Server provided no host key".to_string()))?;

    match resolve_known_host_check_result_with_known_hosts_fallback(
        &known_hosts,
        host,
        port,
        key,
        &kh_path,
    ) {
        ssh2::CheckResult::Match => Ok(()),
        ssh2::CheckResult::Mismatch => Err(SshError::HostKeyMismatch {
            host: host.to_string(),
            port,
        }),
        ssh2::CheckResult::NotFound | ssh2::CheckResult::Failure => {
            let fingerprint = sha256_fingerprint(key);
            match host_key_cb(&fingerprint, host, port) {
                HostKeyDecision::Reject => Err(SshError::ConnectionFailed(format!(
                    "Host key rejected for {host}:{port}"
                ))),
                HostKeyDecision::Accept => {
                    ensure_ssh_dir()?;
                    let known_host = known_hosts_entry_host(host, port);
                    known_hosts
                        .add(&known_host, key, "", host_key_type_to_format(key_type))
                        .map_err(|e| {
                            SshError::ConnectionFailed(format!("Failed to add host key: {e}"))
                        })?;
                    known_hosts
                        .write_file(&kh_path, ssh2::KnownHostFileKind::OpenSSH)
                        .map_err(|e| {
                            SshError::ConnectionFailed(format!("Failed to write known_hosts: {e}"))
                        })?;
                    set_file_permissions_600(&kh_path);
                    Ok(())
                }
            }
        }
    }
}

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
fn resolve_known_host_check_result_with_known_hosts_fallback(
    known_hosts: &ssh2::KnownHosts,
    host: &str,
    port: u16,
    key: &[u8],
    known_hosts_path: &Path,
) -> ssh2::CheckResult {
    let primary = resolve_known_host_check_result(known_hosts, host, port, key);
    if matches!(primary, ssh2::CheckResult::Match) {
        return primary;
    }

    let fallback = check_known_hosts_with_parser(host, port, key, known_hosts_path);
    match fallback {
        Some(ssh2::CheckResult::Match) => ssh2::CheckResult::Match,
        Some(ssh2::CheckResult::Mismatch) => ssh2::CheckResult::Mismatch,
        Some(ssh2::CheckResult::NotFound) => {
            if matches!(primary, ssh2::CheckResult::Failure) {
                ssh2::CheckResult::NotFound
            } else {
                primary
            }
        }
        Some(ssh2::CheckResult::Failure) | None => primary,
    }
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
/// - `Some(ssh2::CheckResult)`: Parsed result from known-host entries.
/// - `None`: The file could not be parsed or the key format is unsupported.
fn check_known_hosts_with_parser(
    host: &str,
    port: u16,
    key: &[u8],
    known_hosts_path: &Path,
) -> Option<ssh2::CheckResult> {
    let entries = KnownHosts::read_file(known_hosts_path).ok()?;
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
/// - `ssh2::CheckResult::Match`: A matching host entry with an identical key was found.
/// - `ssh2::CheckResult::Mismatch`: Host entry exists but key differs.
/// - `ssh2::CheckResult::NotFound`: No host entry matched.
fn resolve_known_host_check_result_from_entries(
    entries: &[KnownHostEntry],
    host: &str,
    port: u16,
    server_key: &PublicKey,
) -> ssh2::CheckResult {
    let mut saw_host_entry = false;
    for entry in entries {
        if !known_host_entry_matches_target(entry, host, port) {
            continue;
        }

        saw_host_entry = true;
        if entry.public_key().key_data() == server_key.key_data() {
            return ssh2::CheckResult::Match;
        }
    }

    if saw_host_entry {
        ssh2::CheckResult::Mismatch
    } else {
        ssh2::CheckResult::NotFound
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

/// Build the host string format used when storing entries in `known_hosts`.
///
/// ### Arguments
/// - `host`: Hostname or IP.
/// - `port`: SSH port.
///
/// ### Returns
/// - `String`: Plain host for port 22, or bracketed `[host]:port` for non-default ports.
fn known_hosts_entry_host(host: &str, port: u16) -> String {
    if port == 22 {
        host.to_string()
    } else {
        format!("[{host}]:{port}")
    }
}

/// Compute a colon-separated SHA-256 hex fingerprint from raw host-key bytes.
///
/// ### Arguments
/// - `key`: Raw bytes of the server's host key.
///
/// ### Returns
/// - `String`: Hex pairs joined by colons, e.g. `"ab:cd:ef:…"`.
fn sha256_fingerprint(key: &[u8]) -> String {
    let hash = Sha256::digest(key);
    hash.iter()
        .map(|b| format!("{b:02x}"))
        .collect::<Vec<_>>()
        .join(":")
}

/// Map a `ssh2::HostKeyType` to the `KnownHostKeyFormat` required by `known_hosts.add`.
///
/// ### Arguments
/// - `key_type`: Key type reported by libssh2 after handshake.
///
/// ### Returns
/// - `ssh2::KnownHostKeyFormat`: Corresponding format constant; `Unknown` falls back to `SshRsa`.
fn host_key_type_to_format(key_type: ssh2::HostKeyType) -> ssh2::KnownHostKeyFormat {
    match key_type {
        ssh2::HostKeyType::Dss => ssh2::KnownHostKeyFormat::SshDss,
        ssh2::HostKeyType::Ecdsa256 => ssh2::KnownHostKeyFormat::Ecdsa256,
        ssh2::HostKeyType::Ecdsa384 => ssh2::KnownHostKeyFormat::Ecdsa384,
        ssh2::HostKeyType::Ecdsa521 => ssh2::KnownHostKeyFormat::Ecdsa521,
        ssh2::HostKeyType::Ed25519 => ssh2::KnownHostKeyFormat::Ed25519,
        ssh2::HostKeyType::Rsa | ssh2::HostKeyType::Unknown => ssh2::KnownHostKeyFormat::SshRsa,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_check_results, known_hosts_entry_host,
        resolve_known_host_check_result_from_entries,
    };
    use ssh_key::{PublicKey, known_hosts::KnownHosts};

    fn parse_known_host_entries(input: &str) -> Vec<ssh_key::known_hosts::Entry> {
        KnownHosts::new(input)
            .collect::<ssh_key::Result<Vec<_>>>()
            .expect("failed to parse known_hosts entries")
    }

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
    fn known_hosts_entry_uses_plain_host_for_default_port() {
        assert_eq!(known_hosts_entry_host("example.com", 22), "example.com");
    }

    #[test]
    fn known_hosts_entry_uses_bracket_host_for_custom_port() {
        assert_eq!(
            known_hosts_entry_host("example.com", 2222),
            "[example.com]:2222"
        );
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
        assert!(matches!(result, ssh2::CheckResult::Match));
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
        assert!(matches!(result, ssh2::CheckResult::Mismatch));
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
        assert!(matches!(result, ssh2::CheckResult::Match));
    }
}
