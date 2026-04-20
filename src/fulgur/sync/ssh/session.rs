use super::error::SshError;
use super::url::RemoteSpec;
use sha2::{Digest, Sha256};
use ssh2::Session;
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::time::Duration;
use zeroize::Zeroizing;

const CONNECT_TIMEOUT_SECS: u64 = 10;
const SESSION_TIMEOUT_MS: u32 = 30_000;

/// An established SSH session with an open SFTP subsystem.
///
/// `Session` and `Sftp` are not `Send`, so this struct must stay within the thread that created it.
pub struct SshSession {
    pub session: Session,
    pub sftp: ssh2::Sftp,
}

/// Decision returned by the host-key callback when a server is not yet in `known_hosts`.
pub enum HostKeyDecision {
    Accept,
    Reject,
}

/// Connect to a remote host over SSH and open an SFTP subsystem.
///
/// ### Description
/// Performs TCP connect (10 s timeout), SSH handshake, host-key verification against
/// `~/.ssh/known_hosts` (TOFU policy), password authentication, and SFTP subsystem init.
/// `host_key_cb` is called synchronously when the server's key is not yet in `known_hosts`;
/// on `Accept` the key is appended to the file.
///
/// ### Arguments
/// - `spec`: Parsed remote specification supplying host and port.
/// - `user`: Resolved username; must not be empty.
/// - `password`: Session-scoped password, zeroed on drop by the `Zeroizing` wrapper.
/// - `host_key_cb`: Called with `(fingerprint_sha256_hex, host, port)` when the host key is unknown.
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
    let addr = addr_str
        .to_socket_addrs()
        .map_err(|e| SshError::ConnectionFailed(format!("Cannot resolve {}: {}", addr_str, e)))?
        .next()
        .ok_or_else(|| {
            SshError::ConnectionFailed(format!("No addresses found for {}", addr_str))
        })?;

    let tcp = TcpStream::connect_timeout(&addr, Duration::from_secs(CONNECT_TIMEOUT_SECS))
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    let mut session = Session::new().map_err(|e| SshError::ConnectionFailed(e.to_string()))?;
    session.set_timeout(SESSION_TIMEOUT_MS);
    session.set_tcp_stream(tcp);
    session
        .handshake()
        .map_err(|e| SshError::ConnectionFailed(e.to_string()))?;

    check_host_key(&session, &spec.host, spec.port, host_key_cb)?;

    session
        .userauth_password(user, password.as_str())
        .map_err(|_| SshError::AuthFailed)?;

    if !session.authenticated() {
        return Err(SshError::AuthFailed);
    }

    let sftp = session
        .sftp()
        .map_err(|e| SshError::SftpError(e.to_string()))?;

    Ok(SshSession { session, sftp })
}

/// Verify the server's host key against `~/.ssh/known_hosts`.
///
/// ### Description
/// Match → proceed silently. Mismatch → hard error (user must edit `known_hosts` manually).
/// NotFound/Failure → call `host_key_cb`; on `Accept` append the entry to the file.
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
fn check_host_key(
    session: &Session,
    host: &str,
    port: u16,
    host_key_cb: impl FnOnce(&str, &str, u16) -> HostKeyDecision,
) -> Result<(), SshError> {
    let kh_path = known_hosts_path();
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

    match known_hosts.check_port(host, port, key) {
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
                    known_hosts
                        .add(host, key, "", host_key_type_to_format(key_type))
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
        ssh2::HostKeyType::Rsa => ssh2::KnownHostKeyFormat::SshRsa,
        ssh2::HostKeyType::Dss => ssh2::KnownHostKeyFormat::SshDss,
        ssh2::HostKeyType::Ecdsa256 => ssh2::KnownHostKeyFormat::Ecdsa256,
        ssh2::HostKeyType::Ecdsa384 => ssh2::KnownHostKeyFormat::Ecdsa384,
        ssh2::HostKeyType::Ecdsa521 => ssh2::KnownHostKeyFormat::Ecdsa521,
        ssh2::HostKeyType::Ed25519 => ssh2::KnownHostKeyFormat::Ed25519,
        ssh2::HostKeyType::Unknown => ssh2::KnownHostKeyFormat::SshRsa,
    }
}

/// Return the platform-appropriate path to `~/.ssh/known_hosts`.
///
/// ### Returns
/// - `PathBuf`: Absolute path derived from `home_dir()`.
fn known_hosts_path() -> PathBuf {
    home_dir().join(".ssh").join("known_hosts")
}

/// Create `~/.ssh` with mode `0700` on Unix if it does not already exist.
///
/// ### Returns
/// - `Ok(())`: Directory exists or was created successfully.
/// - `Err(SshError::IoError)`: Directory could not be created.
fn ensure_ssh_dir() -> Result<(), SshError> {
    let ssh_dir = home_dir().join(".ssh");
    if !ssh_dir.exists() {
        std::fs::create_dir_all(&ssh_dir)
            .map_err(|e| SshError::IoError(format!("Failed to create ~/.ssh: {e}")))?;
        set_dir_permissions_700(&ssh_dir);
    }
    Ok(())
}

/// Return the current user's home directory from platform-specific environment variables.
///
/// ### Returns
/// - `PathBuf`: Home directory path; falls back to `/tmp` on Unix or `C:\Users\User` on Windows
///   if the environment variable is missing.
pub fn home_dir() -> PathBuf {
    #[cfg(windows)]
    {
        std::env::var("USERPROFILE")
            .or_else(|_| {
                std::env::var("HOMEDRIVE")
                    .and_then(|d| std::env::var("HOMEPATH").map(|p| format!("{d}{p}")))
            })
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from(r"C:\Users\User"))
    }
    #[cfg(not(windows))]
    {
        std::env::var("HOME")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("/tmp"))
    }
}

/// Set file permissions to `0600` on Unix; no-op on Windows.
///
/// ### Arguments
/// - `path`: Path to the file whose permissions are to be set.
fn set_file_permissions_600(path: &Path) {
    #[cfg(unix)]
    apply_unix_mode(path, 0o600);
    #[cfg(not(unix))]
    let _ = path;
}

/// Set directory permissions to `0700` on Unix; no-op on Windows.
///
/// ### Arguments
/// - `path`: Path to the directory whose permissions are to be set.
fn set_dir_permissions_700(path: &Path) {
    #[cfg(unix)]
    apply_unix_mode(path, 0o700);
    #[cfg(not(unix))]
    let _ = path;
}

/// Apply a Unix permission mode to a file or directory.
///
/// ### Arguments
/// - `path`: Path to the filesystem entry.
/// - `mode`: Unix permission bits, e.g. `0o600` or `0o700`.
#[cfg(unix)]
fn apply_unix_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    if let Ok(meta) = std::fs::metadata(path) {
        let mut perms = meta.permissions();
        perms.set_mode(mode);
        let _ = std::fs::set_permissions(path, perms);
    }
}
