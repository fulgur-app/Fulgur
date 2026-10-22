pub mod auth;
pub mod credentials;
pub mod error;
pub mod pool;
pub mod save_queue;
pub mod session;
pub mod sftp;
pub mod url;

/// Canonical root directory marker in the remote SFTP namespace.
pub const REMOTE_ROOT_PATH: &str = "/";

/// Port assumed by `RemoteSpec` when the URL does not name one.
pub const DEFAULT_SSH_PORT: u16 = 22;

pub use session::{HostKeyDecision, HostKeyRequest};
