use std::fmt;

/// All errors that can arise from SSH/SFTP operations.
#[derive(Debug)]
pub enum SshError {
    /// A user-supplied URL could not be parsed into a `RemoteSpec`.
    ParseError(String),
    /// TCP connection or SSH handshake failed.
    ConnectionFailed(String),
    /// Server's host key is in `known_hosts` but does not match.
    HostKeyMismatch { host: String, port: u16 },
    /// Server's host key is not in `known_hosts`; contains the SHA-256 fingerprint for display.
    UnknownHost {
        host: String,
        port: u16,
        fingerprint: String,
    },
    /// Password authentication was rejected by the server.
    AuthFailed,
    /// An SFTP-level error (open, read, write, rename, or unlink).
    SftpError(String),
    /// Local filesystem I/O error, e.g. when reading or writing `known_hosts`.
    IoError(String),
}

impl fmt::Display for SshError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SshError::ParseError(msg) => write!(f, "Invalid remote URL: {msg}"),
            SshError::ConnectionFailed(msg) => write!(f, "Connection failed: {msg}"),
            SshError::HostKeyMismatch { host, port } => write!(
                f,
                "Host key mismatch for {host}:{port} — possible MITM. \
                 Verify or update ~/.ssh/known_hosts."
            ),
            SshError::UnknownHost {
                host,
                port,
                fingerprint,
            } => write!(f, "Unknown host {host}:{port} (fingerprint: {fingerprint})"),
            SshError::AuthFailed => {
                write!(f, "Authentication failed — check username and password.")
            }
            SshError::SftpError(msg) => write!(f, "SFTP error: {msg}"),
            SshError::IoError(msg) => write!(f, "I/O error: {msg}"),
        }
    }
}

impl SshError {
    /// Return the error as a human-readable string suitable for displaying in a notification.
    ///
    /// ### Returns
    /// - `String`: User-facing message; delegates to the `Display` implementation.
    pub fn user_message(&self) -> String {
        self.to_string()
    }
}
