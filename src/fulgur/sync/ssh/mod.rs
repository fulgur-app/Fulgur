pub mod credentials;
pub mod error;
pub mod pool;
pub mod session;
pub mod sftp;
pub mod url;

/// Canonical root directory marker in the remote SFTP namespace.
pub const REMOTE_ROOT_PATH: &str = "/";

pub use session::{HostKeyDecision, HostKeyRequest};
