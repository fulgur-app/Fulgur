mod authenticate;
mod host_key;
mod host_patterns;
mod identities;
mod paths;
mod ssh_config;

pub use identities::expand_tilde;
pub use paths::home_dir;
pub use ssh_config::SshHostConfig;

use super::auth::{CredentialPrompter, SshAuth};
use super::error::SshError;
use super::url::{RemoteSpec, format_host_for_url};
use authenticate::authenticate;
use host_key::check_host_key;
use host_patterns::hostkey_method_preferences_from_known_hosts;
use ssh2::Session;
use std::fmt;
use std::net::{TcpStream, ToSocketAddrs};
use std::time::Duration;

const CONNECT_TIMEOUT_SECS: u64 = 10;
const SESSION_TIMEOUT_MS: u32 = 30_000;
const SESSION_CLOSE_TIMEOUT_MS: u32 = 2_000;

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

impl Drop for SshSession {
    fn drop(&mut self) {
        self.session.set_timeout(SESSION_CLOSE_TIMEOUT_MS);
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
/// - `preferred_auth`: Credentials that authenticated this target before, tried first.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the host key is unknown.
/// - `prompter`: Source of passphrases, passwords, and key files when automatic methods fail.
///
/// ### Errors
/// Returns an `SshError` on TCP connect failure, SSH handshake failure, host-key
/// rejection, authentication failure or cancellation, or SFTP subsystem init failure.
///
/// ### Returns
/// - `Ok((SshSession, SshAuth))`: Ready session with an open SFTP subsystem, and the
///   credentials that authenticated it.
/// - `Err(SshError)`: Any failure during TCP connect, handshake, host-key check, auth, or SFTP init.
pub fn connect(
    spec: &RemoteSpec,
    user: &str,
    preferred_auth: Option<&SshAuth>,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
    prompter: &mut dyn CredentialPrompter,
) -> Result<(SshSession, SshAuth), SshError> {
    let host_config = SshHostConfig::load(&spec.host);
    let (host, port) = host_config.connection_target(&spec.host, spec.port);
    let target = ConnectionTarget {
        alias: &spec.host,
        host,
        port,
        user,
    };
    log::debug!("SSH connecting to {target}");
    let result = establish_session(&target, &host_config, preferred_auth, host_key_cb, prompter);
    match &result {
        Ok((_, auth)) => log::info!(
            "SSH connected to {target} (authentication: {})",
            auth.method_label()
        ),
        Err(SshError::AuthCancelled) => log::info!("SSH connection to {target} cancelled"),
        Err(error) => log::warn!("SSH connection to {target} failed: {error}"),
    }
    result
}

/// Address a connection goes to, as shown in the logs.
struct ConnectionTarget<'a> {
    /// Host as typed in the remote URL, possibly a `~/.ssh/config` alias.
    alias: &'a str,
    /// Hostname the TCP connection is opened to.
    host: String,
    port: u16,
    user: &'a str,
}

impl fmt::Display for ConnectionTarget<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let host = format_host_for_url(&self.host);
        write!(f, "{}@{host}:{}", self.user, self.port)?;
        if self.alias != self.host {
            write!(f, " (alias {})", self.alias)?;
        }
        Ok(())
    }
}

/// Run every connection step: TCP, handshake, host-key check, authentication, and SFTP.
///
/// ### Arguments
/// - `target`: Where to connect, and as whom.
/// - `host_config`: Settings from `~/.ssh/config` for the URL host.
/// - `preferred_auth`: Credentials that authenticated this target before, tried first.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the host key is unknown.
/// - `prompter`: Source of passphrases, passwords, and key files when automatic methods fail.
///
/// ### Errors
/// Returns an `SshError` on the first step that fails.
///
/// ### Returns
/// - `Ok((SshSession, SshAuth))`: Ready session and the credentials that authenticated it.
/// - `Err(SshError)`: The step that failed and why.
fn establish_session(
    target: &ConnectionTarget<'_>,
    host_config: &SshHostConfig,
    preferred_auth: Option<&SshAuth>,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
    prompter: &mut dyn CredentialPrompter,
) -> Result<(SshSession, SshAuth), SshError> {
    let tcp = open_tcp_stream(&target.host, target.port)?;

    let mut session = Session::new().map_err(|e| SshError::ConnectionFailed(e.to_string()))?;
    if let Some(hostkey_prefs) =
        hostkey_method_preferences_from_known_hosts(&target.host, target.port)
    {
        let _ = session.method_pref(ssh2::MethodType::HostKey, &hostkey_prefs);
    }
    session.set_timeout(SESSION_TIMEOUT_MS);
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;
    log::debug!("SSH handshake with {target} completed");

    check_host_key(&session, &target.host, target.port, host_key_cb)?;

    let home = home_dir().ok();
    let auth = authenticate(
        &session,
        target.alias,
        host_config,
        target.user,
        home.as_deref(),
        preferred_auth,
        prompter,
    )?;
    if !session.authenticated() {
        return Err(SshError::AuthFailed);
    }

    let sftp = session
        .sftp()
        .map_err(|e| SshError::SftpError(e.to_string()))?;

    Ok((SshSession { session, sftp }, auth))
}

/// Open a TCP connection to the first reachable address of a host.
///
/// Addresses are tried in resolver order: DNS may return IPv6 before IPv4, and falling
/// through on `EHOSTUNREACH` lets the connection succeed when only one family is routable.
///
/// ### Arguments
/// - `host`: Hostname or IP address.
/// - `port`: TCP port.
///
/// ### Errors
/// Returns `SshError::ConnectionFailed` when the host does not resolve or no address accepts
/// the connection.
///
/// ### Returns
/// - `Ok(TcpStream)`: The connected stream.
/// - `Err(SshError)`: The resolution error, or the error of the last address tried.
fn open_tcp_stream(host: &str, port: u16) -> Result<TcpStream, SshError> {
    let addr_str = format!("{host}:{port}");
    let addrs: Vec<_> = addr_str
        .to_socket_addrs()
        .map_err(|e| SshError::ConnectionFailed(format!("Cannot resolve {addr_str}: {e}")))?
        .collect();
    if addrs.is_empty() {
        return Err(SshError::ConnectionFailed(format!(
            "No addresses found for {addr_str}"
        )));
    }
    let mut last_err = None;
    addrs
        .into_iter()
        .find_map(|addr| {
            match TcpStream::connect_timeout(&addr, Duration::from_secs(CONNECT_TIMEOUT_SECS)) {
                Ok(stream) => {
                    log::debug!("TCP connection to {addr} established");
                    Some(stream)
                }
                Err(e) => {
                    log::debug!("TCP connection to {addr} failed: {e}");
                    last_err = Some(e);
                    None
                }
            }
        })
        .ok_or_else(|| {
            SshError::ConnectionFailed(
                last_err.map_or_else(|| "unknown error".to_string(), |e| e.to_string()),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::ConnectionTarget;

    #[test]
    fn connection_target_names_the_alias_only_when_it_was_resolved() {
        let direct = ConnectionTarget {
            alias: "192.168.1.152",
            host: "192.168.1.152".to_string(),
            port: 22,
            user: "alice",
        };
        let aliased = ConnectionTarget {
            alias: "nas",
            host: "192.168.1.152".to_string(),
            port: 2222,
            user: "alice",
        };
        assert_eq!(direct.to_string(), "alice@192.168.1.152:22");
        assert_eq!(aliased.to_string(), "alice@192.168.1.152:2222 (alias nas)");
    }

    #[test]
    fn connection_target_brackets_ipv6_hosts() {
        let target = ConnectionTarget {
            alias: "::1",
            host: "::1".to_string(),
            port: 22,
            user: "alice",
        };
        assert_eq!(target.to_string(), "alice@[::1]:22");
    }
}
