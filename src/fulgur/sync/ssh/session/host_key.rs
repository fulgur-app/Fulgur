use super::super::error::SshError;
use super::host_patterns::known_host_entry_matches_target;
use super::paths::{ensure_ssh_dir, known_hosts_path, set_file_permissions_600};
use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use sha2::{Digest, Sha256};
use ssh_key::{
    PublicKey,
    known_hosts::{Entry as KnownHostEntry, KnownHosts},
};
use ssh2::Session;
use std::fs::{File, OpenOptions};
use std::io::{ErrorKind, Read, Seek, SeekFrom, Write};
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

    let (key, _) = session
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
                    append_known_hosts_entry(&kh_path, &known_host, key)?;
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

/// Append one OpenSSH-format entry to `known_hosts`, leaving existing lines untouched.
///
/// ### Arguments
/// - `known_hosts_path`: Path to the `known_hosts` file, created if it does not exist.
/// - `known_host`: Host pattern to store, as produced by `known_hosts_entry_host`.
/// - `key`: Raw server host key in SSH wire format.
///
/// ### Errors
/// - Returns `SshError::ConnectionFailed` if the key blob is malformed or the file cannot be
///   opened, inspected, or written.
///
/// ### Returns
/// - `Ok(())`: The entry was appended.
/// - `Err(SshError::ConnectionFailed)`: The key was unusable or the file could not be updated.
fn append_known_hosts_entry(
    known_hosts_path: &Path,
    known_host: &str,
    key: &[u8],
) -> Result<(), SshError> {
    let entry = known_hosts_entry_line(known_host, key)?;
    let separator = if known_hosts_ends_mid_line(known_hosts_path)? {
        "\n"
    } else {
        ""
    };

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(known_hosts_path)
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to open known_hosts: {e}")))?;
    file.write_all(format!("{separator}{entry}\n").as_bytes())
        .map_err(|e| SshError::ConnectionFailed(format!("Failed to write known_hosts: {e}")))
}

/// Build a single OpenSSH `known_hosts` line for a host pattern and a raw host key.
///
/// ### Arguments
/// - `known_host`: Host pattern to store, as produced by `known_hosts_entry_host`.
/// - `key`: Raw server host key in SSH wire format.
///
/// ### Errors
/// - Returns `SshError::ConnectionFailed` if the algorithm name cannot be read from the key.
///
/// ### Returns
/// - `Ok(String)`: The `<host> <algorithm> <base64-key>` line, without a trailing newline.
/// - `Err(SshError::ConnectionFailed)`: The key blob is not in SSH wire format.
fn known_hosts_entry_line(known_host: &str, key: &[u8]) -> Result<String, SshError> {
    let algorithm = host_key_algorithm_name(key).ok_or_else(|| {
        SshError::ConnectionFailed(
            "Server host key is not in a recognizable SSH wire format".to_string(),
        )
    })?;
    Ok(format!("{known_host} {algorithm} {}", BASE64.encode(key)))
}

/// Read the algorithm name embedded at the start of an SSH wire-format public key.
///
/// ### Arguments
/// - `key`: Raw server host key in SSH wire format.
///
/// ### Returns
/// - `Some(&str)`: The algorithm name, for example `"ssh-ed25519"`.
/// - `None`: The blob is truncated or the name is not a printable ASCII token.
fn host_key_algorithm_name(key: &[u8]) -> Option<&str> {
    let length_prefix: [u8; 4] = key.get(..4)?.try_into().ok()?;
    let name_length = usize::try_from(u32::from_be_bytes(length_prefix)).ok()?;
    let name = std::str::from_utf8(key.get(4..)?.get(..name_length)?).ok()?;
    let is_single_token = !name.is_empty() && name.chars().all(|c| c.is_ascii_graphic());
    is_single_token.then_some(name)
}

/// Report whether `known_hosts` ends without a trailing newline.
///
/// ### Arguments
/// - `known_hosts_path`: Path to the `known_hosts` file.
///
/// ### Errors
/// - Returns `SshError::ConnectionFailed` if the file exists but cannot be inspected.
///
/// ### Returns
/// - `Ok(true)`: The file has content whose last byte is not a newline, so an appended entry
///   must be preceded by one.
/// - `Ok(false)`: The file is missing, empty, or already newline-terminated.
/// - `Err(SshError::ConnectionFailed)`: The file could not be opened or read.
fn known_hosts_ends_mid_line(known_hosts_path: &Path) -> Result<bool, SshError> {
    let mut file = match File::open(known_hosts_path) {
        Ok(file) => file,
        Err(e) if e.kind() == ErrorKind::NotFound => return Ok(false),
        Err(e) => {
            return Err(SshError::ConnectionFailed(format!(
                "Failed to open known_hosts: {e}"
            )));
        }
    };

    let read_error =
        |e: std::io::Error| SshError::ConnectionFailed(format!("Failed to read known_hosts: {e}"));
    if file.seek(SeekFrom::End(0)).map_err(read_error)? == 0 {
        return Ok(false);
    }

    file.seek(SeekFrom::End(-1)).map_err(read_error)?;
    let mut last_byte = [0_u8; 1];
    file.read_exact(&mut last_byte).map_err(read_error)?;
    Ok(last_byte[0] != b'\n')
}

#[cfg(test)]
mod tests {
    use super::{
        aggregate_check_results, append_known_hosts_entry, host_key_algorithm_name,
        known_hosts_entry_host, known_hosts_entry_line,
        resolve_known_host_check_result_from_entries,
    };
    use ssh_key::{PublicKey, known_hosts::KnownHosts};

    const SERVER_KEY_OPENSSH: &str =
        "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ server";

    fn parse_known_host_entries(input: &str) -> Vec<ssh_key::known_hosts::Entry> {
        KnownHosts::new(input)
            .collect::<ssh_key::Result<Vec<_>>>()
            .expect("failed to parse known_hosts entries")
    }

    fn server_key() -> PublicKey {
        PublicKey::from_openssh(SERVER_KEY_OPENSSH).expect("failed to parse server key")
    }

    fn server_key_bytes() -> Vec<u8> {
        server_key()
            .to_bytes()
            .expect("failed to encode server key to wire format")
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

    #[test]
    fn algorithm_name_is_read_from_the_wire_format_prefix() {
        let key = server_key_bytes();
        assert_eq!(host_key_algorithm_name(&key), Some("ssh-ed25519"));
    }

    #[test]
    fn algorithm_name_rejects_a_truncated_key() {
        assert_eq!(host_key_algorithm_name(&[0, 0, 0, 11, b's']), None);
    }

    #[test]
    fn algorithm_name_rejects_a_non_printable_name() {
        assert_eq!(
            host_key_algorithm_name(&[0, 0, 0, 3, b'a', b' ', b'b']),
            None
        );
    }

    #[test]
    fn entry_line_uses_the_openssh_host_algorithm_key_layout() {
        let line = known_hosts_entry_line("[example.com]:2222", &server_key_bytes())
            .expect("failed to build known_hosts line");

        let mut fields = line.split(' ');
        assert_eq!(fields.next(), Some("[example.com]:2222"));
        assert_eq!(fields.next(), Some("ssh-ed25519"));
        assert_eq!(
            fields.next(),
            Some("AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ")
        );
        assert_eq!(fields.next(), None);
    }

    #[test]
    fn appending_preserves_lines_a_parser_cannot_represent() {
        let existing = "# a comment libssh2 drops\n\
             @cert-authority *.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIA6rWI3G1sz07DnfFlrouTcysQlj2P+jpNSOEWD9OJ3X\n\
             @revoked old.example.com ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAILIG2T/B0l0gaqj3puu510tu9N1OkQ4znY3LYuEm5zCF\n";
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, existing).expect("failed to seed known_hosts");

        append_known_hosts_entry(&path, "example.com", &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        assert!(contents.starts_with(existing));
        assert!(contents.ends_with('\n'));
        assert_eq!(contents.lines().count(), 4);
    }

    #[test]
    fn appending_separates_an_entry_from_a_file_ending_mid_line() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");
        std::fs::write(&path, "old.example.com ssh-rsa AAAAB3NzaC1yc2E=")
            .expect("failed to seed known_hosts");

        append_known_hosts_entry(&path, "example.com", &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        assert_eq!(contents.lines().count(), 2);
        assert_eq!(
            contents.lines().next(),
            Some("old.example.com ssh-rsa AAAAB3NzaC1yc2E=")
        );
    }

    #[test]
    fn appending_creates_a_missing_known_hosts_file() {
        let dir = tempfile::tempdir().expect("failed to create temp dir");
        let path = dir.path().join("known_hosts");

        append_known_hosts_entry(&path, "example.com", &server_key_bytes())
            .expect("failed to append known_hosts entry");

        let contents = std::fs::read_to_string(&path).expect("failed to read known_hosts");
        assert_eq!(contents.lines().count(), 1);
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
        assert!(matches!(result, ssh2::CheckResult::Match));
    }
}
