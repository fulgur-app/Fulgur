mod host_key;
mod host_patterns;
mod paths;

pub use paths::home_dir;

use super::error::SshError;
use super::url::RemoteSpec;
use host_key::check_host_key;
use host_patterns::hostkey_method_preferences_from_known_hosts;
use ssh2::Session;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;
use zeroize::Zeroizing;

const CONNECT_TIMEOUT_SECS: u64 = 10;
const SESSION_TIMEOUT_MS: u32 = 30_000;
const LIBSSH2_AUTHENTICATION_FAILED_CODE: i32 = -18;

/// An established SSH session with an open SFTP subsystem.
pub struct SshSession {
    pub session: Session,
    pub sftp: ssh2::Sftp,
}

impl SshSession {
    /// Report whether the underlying ssh2 session still believes it is authenticated.
    ///
    /// ### Returns
    /// - `true`: ssh2 reports the session as authenticated.
    /// - `false`: The session is not authenticated and should be discarded.
    #[must_use]
    pub fn is_authenticated(&self) -> bool {
        self.session.authenticated()
    }
}

/// Decision returned by the host-key callback when a server is not yet in `known_hosts`.
pub enum HostKeyDecision {
    Accept,
    Reject,
}

/// A request posted by the SSH background thread when it encounters a host key not in `known_hosts`.
pub struct HostKeyRequest {
    /// SHA-256 fingerprint of the server's host key, formatted as colon-separated hex pairs.
    pub fingerprint: String,
    /// Hostname of the remote server.
    pub host: String,
    /// SSH port of the remote server.
    pub port: u16,
    /// Channel sender to unblock the SSH thread once the user has decided.
    pub decision_tx: std::sync::mpsc::Sender<HostKeyDecision>,
}

/// Connect to a remote host over SSH and open an SFTP subsystem.
///
/// ### Arguments
/// - `spec`: Parsed remote specification supplying host and port.
/// - `user`: Resolved username; must not be empty.
/// - `password`: Session-scoped password, zeroed on drop by the `Zeroizing` wrapper.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the host key is unknown.
///
/// ### Errors
/// Returns an `SshError` on TCP connect failure, SSH handshake failure, host-key
/// rejection, password authentication failure, or SFTP subsystem init failure.
///
/// ### Returns
/// - `Ok(SshSession)`: Ready session with an open SFTP subsystem.
/// - `Err(SshError)`: Any failure during TCP connect, handshake, host-key check, auth, or SFTP init.
pub fn connect(
    spec: &RemoteSpec,
    user: &str,
    password: &Zeroizing<String>,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
) -> Result<SshSession, SshError> {
    let addr_str = format!("{}:{}", spec.host, spec.port);
    let addrs: Vec<_> = addr_str
        .to_socket_addrs()
        .map_err(|e| SshError::ConnectionFailed(format!("Cannot resolve {addr_str}: {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(SshError::ConnectionFailed(format!(
            "No addresses found for {addr_str}"
        )));
    }
    // Try each resolved address in order (DNS may return IPv6 before IPv4; fall through on
    // EHOSTUNREACH so the connection succeeds even when only one address family is routable).
    let mut last_err = None;
    let tcp = addrs
        .into_iter()
        .find_map(|addr| {
            match TcpStream::connect_timeout(&addr, Duration::from_secs(CONNECT_TIMEOUT_SECS)) {
                Ok(stream) => Some(stream),
                Err(e) => {
                    last_err = Some(e);
                    None
                }
            }
        })
        .ok_or_else(|| {
            SshError::ConnectionFailed(
                last_err.map_or_else(|| "unknown error".to_string(), |e| e.to_string()),
            )
        })?;

    let mut session = Session::new().map_err(|e| SshError::ConnectionFailed(e.to_string()))?;
    if let Some(hostkey_prefs) = hostkey_method_preferences_from_known_hosts(&spec.host, spec.port)
    {
        let _ = session.method_pref(ssh2::MethodType::HostKey, &hostkey_prefs);
    }
    session.set_timeout(SESSION_TIMEOUT_MS);
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    check_host_key(&session, &spec.host, spec.port, host_key_cb)?;

    session
        .userauth_password(user, password.as_str())
        .map_err(|e| map_password_auth_error(&e))?;

    if !session.authenticated() {
        return Err(SshError::AuthFailed);
    }

    let sftp = session
        .sftp()
        .map_err(|e| SshError::SftpError(e.to_string()))?;

    Ok(SshSession { session, sftp })
}

/// Classify `userauth_password` failures as credential rejection or transport/session errors.
///
/// ### Arguments
/// - `error`: Raw ssh2 error returned from `userauth_password`.
///
/// ### Returns
/// - `SshError::AuthFailed`: Password authentication was explicitly rejected by the server.
/// - `SshError::ConnectionFailed`: Any non-auth failure during the authentication request.
fn map_password_auth_error(error: &ssh2::Error) -> SshError {
    match error.code() {
        ssh2::ErrorCode::Session(code) if code == LIBSSH2_AUTHENTICATION_FAILED_CODE => {
            SshError::AuthFailed
        }
        _ => SshError::ConnectionFailed(format!("SSH authentication request failed: {error}")),
    }
}

#[cfg(test)]
mod tests {
    use super::map_password_auth_error;
    use crate::fulgur::sync::ssh::error::SshError;

    #[test]
    fn map_password_auth_error_maps_authentication_rejection() {
        let error = ssh2::Error::from_errno(ssh2::ErrorCode::Session(-18));
        assert!(matches!(
            map_password_auth_error(&error),
            SshError::AuthFailed
        ));
    }

    #[test]
    fn map_password_auth_error_maps_non_auth_errors_to_connection_failed() {
        let error = ssh2::Error::from_errno(ssh2::ErrorCode::Session(-7));
        match map_password_auth_error(&error) {
            SshError::ConnectionFailed(message) => {
                assert!(message.contains("SSH authentication request failed"));
            }
            other => panic!("expected ConnectionFailed, got {other:?}"),
        }
    }
}
