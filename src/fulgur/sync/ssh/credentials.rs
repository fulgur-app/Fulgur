use std::collections::HashMap;
use zeroize::Zeroizing;

/// Unique key for a (host, port, user) triple used to look up cached passwords.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct SshCredKey {
    pub host: String,
    pub port: u16,
    pub user: String,
}

impl SshCredKey {
    /// Create a new credential key from its components.
    ///
    /// ### Arguments
    /// - `host`: Hostname or IP address of the remote server.
    /// - `port`: SSH port.
    /// - `user`: Username for authentication.
    pub fn new(host: impl Into<String>, port: u16, user: impl Into<String>) -> Self {
        Self {
            host: host.into(),
            port,
            user: user.into(),
        }
    }

    /// Return a display string in `user@host:port` format.
    ///
    /// ### Returns
    /// - `String`: Human-readable representation, e.g. `"alice@example.com:22"`.
    pub fn display(&self) -> String {
        format!("{}@{}:{}", self.user, self.host, self.port)
    }
}

/// Session-scoped in-memory password cache.
///
/// Stored inside `SharedAppState` behind an `Arc<Mutex<…>>`. Passwords are held as
/// `Zeroizing<String>` so memory is zeroed on drop. The map is never persisted to disk.
pub type SshCredentialCache = HashMap<SshCredKey, Zeroizing<String>>;
