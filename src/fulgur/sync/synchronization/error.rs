use std::fmt;

/// Handle ureq errors and convert them to `SynchronizationError` with appropriate logging
///
/// ### Arguments
/// - `error`: The ureq error to handle
/// - `context`: Human-readable context for logging (e.g., "Failed to get devices")
///
/// ### Returns
/// - `SynchronizationError`: The mapped synchronization error
#[must_use]
pub fn handle_ureq_error(error: ureq::Error, context: &str) -> SynchronizationError {
    match error {
        ureq::Error::StatusCode(code) => {
            log::error!("{context}: HTTP status {code}");
            if code == 401 || code == 403 {
                SynchronizationError::AuthenticationFailed
            } else if code == 400 {
                SynchronizationError::BadRequest
            } else if code == 413 {
                SynchronizationError::ContentTooLarge {
                    file_size: 0,
                    max_size: 0,
                }
            } else {
                SynchronizationError::ServerError(code)
            }
        }
        ureq::Error::Io(io_error) => {
            log::error!("{context} (IO): {io_error}");
            match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => SynchronizationError::ConnectionFailed,
                std::io::ErrorKind::TimedOut => SynchronizationError::Timeout(io_error.to_string()),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted => {
                    SynchronizationError::ConnectionFailed
                }
                std::io::ErrorKind::AddrNotAvailable => SynchronizationError::HostNotFound,
                _ => SynchronizationError::Other(io_error.to_string()),
            }
        }
        ureq::Error::ConnectionFailed => {
            log::error!("{context}: Connection failed");
            SynchronizationError::ConnectionFailed
        }
        ureq::Error::HostNotFound => {
            log::error!("{context}: Host not found");
            SynchronizationError::HostNotFound
        }
        ureq::Error::Timeout(timeout) => {
            log::error!("{context}: Timeout ({timeout})");
            SynchronizationError::Timeout(timeout.to_string())
        }
        e => {
            log::error!("{context}: {e}");
            SynchronizationError::Other(e.to_string())
        }
    }
}

#[derive(Debug)]
pub enum SynchronizationError {
    AuthenticationFailed,
    BadRequest,
    CompressionFailed,
    ConnectionFailed,
    ContentMissing,
    ContentTooLarge { file_size: usize, max_size: usize },
    DeviceIdsMissing,
    DeviceKeyMissing,
    EmailMissing,
    EncryptionFailed,
    FileNameMissing,
    HostNotFound,
    InvalidPublicKey(String),
    InvalidResponse(String),
    MissingEncryptionKey,
    MissingPublicKey(String),
    MissingExpirationDate,
    Other(String),
    ServerError(u16),
    ServerUrlMissing,
    Timeout(String),
}

impl fmt::Display for SynchronizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynchronizationError::AuthenticationFailed => write!(f, "Authentication failed"),
            SynchronizationError::BadRequest => write!(f, "Bad request"),
            SynchronizationError::CompressionFailed => write!(f, "Compression failed"),
            SynchronizationError::ConnectionFailed => write!(f, "Cannot connect to sync server"),
            SynchronizationError::ContentMissing => write!(f, "Content is missing"),
            SynchronizationError::ContentTooLarge {
                file_size: 0,
                max_size: 0,
            } => write!(f, "File is too large to share (rejected by server)"),
            SynchronizationError::ContentTooLarge {
                file_size,
                max_size,
            } => write!(
                f,
                "File is too large to share ({} KB, max {} KB)",
                file_size / 1024,
                max_size / 1024
            ),
            SynchronizationError::DeviceIdsMissing => write!(f, "Device IDs are missing"),
            SynchronizationError::DeviceKeyMissing => write!(f, "Key is missing"),
            SynchronizationError::EmailMissing => write!(f, "Email is missing"),
            SynchronizationError::EncryptionFailed => write!(f, "Encryption failed"),
            SynchronizationError::FileNameMissing => write!(f, "File name is missing"),
            SynchronizationError::HostNotFound => write!(f, "Host not found"),
            SynchronizationError::InvalidPublicKey(name) => {
                write!(f, "Invalid public key for device: {name}")
            }
            SynchronizationError::InvalidResponse(e) | SynchronizationError::Other(e) => {
                write!(f, "{e}")
            }
            SynchronizationError::MissingEncryptionKey => write!(f, "Missing encryption key"),
            SynchronizationError::MissingExpirationDate => write!(f, "Missing expiration date"),
            SynchronizationError::MissingPublicKey(e) => {
                write!(f, "Missing public key for device: {e}")
            }
            SynchronizationError::ServerError(e) => write!(f, "{e}"),
            SynchronizationError::ServerUrlMissing => write!(f, "Server URL is missing"),
            SynchronizationError::Timeout(timeout) => write!(f, "Timeout: {timeout}"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum SynchronizationStatus {
    Connected,
    Connecting,
    Disconnected,
    AuthenticationFailed,
    ConnectionFailed,
    Other,
    NotActivated,
}

impl SynchronizationStatus {
    /// Convert the error to a synchronization status
    ///
    /// ### Arguments
    /// - `error`: The error
    ///
    /// ### Returns
    /// - `SynchronizationStatus`: The synchronization status
    #[must_use]
    pub fn from_error(error: &SynchronizationError) -> SynchronizationStatus {
        match error {
            SynchronizationError::AuthenticationFailed => {
                SynchronizationStatus::AuthenticationFailed
            }
            SynchronizationError::HostNotFound
            | SynchronizationError::ConnectionFailed
            | SynchronizationError::Timeout(_) => SynchronizationStatus::ConnectionFailed,
            _ => SynchronizationStatus::Other,
        }
    }

    /// Check if the synchronization status is connected
    ///
    /// ### Returns
    /// - `true` if the synchronization status is connected, `false` otherwise
    #[must_use]
    pub fn is_connected(&self) -> bool {
        match self {
            SynchronizationStatus::Connected => true,
            SynchronizationStatus::Connecting
            | SynchronizationStatus::Disconnected
            | SynchronizationStatus::AuthenticationFailed
            | SynchronizationStatus::ConnectionFailed
            | SynchronizationStatus::Other
            | SynchronizationStatus::NotActivated => false,
        }
    }

    /// Check if the synchronization status is connecting
    ///
    /// ### Returns
    /// - `true` if the synchronization status is connecting, `false` otherwise
    #[must_use]
    pub fn is_connecting(&self) -> bool {
        matches!(self, SynchronizationStatus::Connecting)
    }

    /// Return a short human-readable label for display in tooltips and status pills.
    ///
    /// ### Returns
    /// - `&'static str`: One of "Connected", "Connecting", "Disconnected", "Inactive", or "Error".
    #[must_use]
    pub fn label(self) -> &'static str {
        match self {
            SynchronizationStatus::Connected => "Connected",
            SynchronizationStatus::Connecting => "Connecting",
            SynchronizationStatus::Disconnected => "Disconnected",
            SynchronizationStatus::NotActivated => "Inactive",
            SynchronizationStatus::AuthenticationFailed
            | SynchronizationStatus::ConnectionFailed
            | SynchronizationStatus::Other => "Error",
        }
    }
}
