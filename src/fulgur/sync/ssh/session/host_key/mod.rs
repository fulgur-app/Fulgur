mod append;
mod lookup;
#[cfg(test)]
mod test_fixtures;

use super::super::error::SshError;
use super::paths::{ensure_ssh_dir, known_hosts_path, set_file_permissions_600};
use append::{append_known_hosts_entry, known_hosts_entry_host};
use lookup::resolve_known_host_check_result_with_known_hosts_fallback;
use sha2::{Digest, Sha256};
use ssh2::Session;

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
        ssh2::CheckResult::Match => {
            log::debug!("Host key of {host}:{port} matches known_hosts");
            Ok(())
        }
        ssh2::CheckResult::Mismatch => Err(SshError::HostKeyMismatch {
            host: host.to_string(),
            port,
        }),
        ssh2::CheckResult::NotFound | ssh2::CheckResult::Failure => {
            let fingerprint = sha256_fingerprint(key);
            log::debug!("Host key of {host}:{port} is unknown, asking the user to trust it");
            match host_key_cb(&fingerprint, host, port) {
                HostKeyDecision::Reject => Err(SshError::ConnectionFailed(format!(
                    "Host key rejected for {host}:{port}"
                ))),
                HostKeyDecision::Accept => {
                    ensure_ssh_dir()?;
                    let known_host = known_hosts_entry_host(host, port);
                    append_known_hosts_entry(&kh_path, &known_host, key)?;
                    set_file_permissions_600(&kh_path);
                    log::info!(
                        "Trusted the host key of {host}:{port} ({fingerprint}) and added it to known_hosts"
                    );
                    Ok(())
                }
            }
        }
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
