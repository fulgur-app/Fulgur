use crate::fulgur::sync::ssh::{
    self, credentials::SshCredKey, pool::SshSessionPool, session::HostKeyDecision,
    sftp::RemoteDirectoryEntry, url::RemoteSpec,
};
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};
use zeroize::Zeroizing;

pub const SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS: u64 = 60;
pub const SSH_HOST_KEY_APPROVAL_TIMEOUT: Duration =
    Duration::from_secs(SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS);
pub const SSH_CONNECTION_TIMEOUT_LABEL: &str = "SSH connection timed out";
pub const SSH_SAVE_TIMEOUT_LABEL: &str = "SSH save timed out";

/// Result of a successfully loaded remote file, delivered by the SSH background thread.
pub struct RemoteFileResult {
    pub spec: RemoteSpec,
    pub content: String,
    pub encoding: String,
    pub file_size: usize,
}

/// Data required to open a remote browsing dialog when the requested path is not a file.
#[derive(Clone)]
pub struct RemoteBrowseResult {
    pub directory_spec: RemoteSpec,
    pub entries: Vec<RemoteDirectoryEntry>,
    pub notice: Option<String>,
}

/// Successful outcomes of a remote open attempt.
pub enum RemoteOpenResult {
    File(RemoteFileResult),
    Browse(RemoteBrowseResult),
}

/// A queued remote-open outcome consumed by `Fulgur::process_pending_remote_files`.
pub struct PendingRemoteOpenOutcome {
    pub target_tab_id: Option<usize>,
    pub target_request_id: Option<u64>,
    pub result: Result<RemoteOpenResult, String>,
}

/// Inputs required to execute a remote open in the SSH worker thread.
pub struct RemoteOpenTaskParams {
    pub spec: RemoteSpec,
    pub password: Zeroizing<String>,
    pub credential_key: SshCredKey,
    pub ssh_session_cache: Arc<Mutex<ssh::credentials::SshCredentialCache>>,
    pub ssh_session_pool: Arc<SshSessionPool>,
    pub target_tab_id: Option<usize>,
    pub target_request_id: Option<u64>,
}

/// Wait for a host-key trust decision with a bounded timeout.
///
/// ### Arguments
/// - `decision_rx`: Receiver used by the host-key dialog to deliver `Accept` or `Reject`
/// - `timed_out`: Shared flag set when the wait elapsed without a decision
///
/// ### Returns
/// - `HostKeyDecision::Accept`: The user accepted the presented host key
/// - `HostKeyDecision::Reject`: The user rejected the key, the channel closed, or timeout elapsed
pub fn wait_for_host_key_decision(
    decision_rx: std::sync::mpsc::Receiver<HostKeyDecision>,
    timed_out: &AtomicBool,
) -> HostKeyDecision {
    match decision_rx.recv_timeout(SSH_HOST_KEY_APPROVAL_TIMEOUT) {
        Ok(decision) => decision,
        Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {
            timed_out.store(true, Ordering::Release);
            HostKeyDecision::Reject
        }
        Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => HostKeyDecision::Reject,
    }
}

/// Build a "Verb to user@host:port" label for progress notifications.
///
/// ### Arguments
/// - `prefix`: Verb prefix ending with a space, e.g. `"Connecting to "`.
/// - `host`: Remote host or IP.
/// - `port`: SSH port.
/// - `user`: Username; an empty string omits the `user@` prefix.
///
/// ### Returns
/// - `String`: Composed label.
pub fn format_remote_endpoint_label(prefix: &str, host: &str, port: u16, user: &str) -> String {
    if user.is_empty() {
        format!("{prefix}{host}:{port}")
    } else {
        format!("{prefix}{user}@{host}:{port}")
    }
}

/// Inputs required to execute a remote save in the SSH worker thread.
pub struct RemoteSaveTaskParams {
    pub tab_id: usize,
    pub request_id: u64,
    pub spec: RemoteSpec,
    pub saved_content: Arc<String>,
    pub password: Zeroizing<String>,
    pub credential_key: SshCredKey,
    pub ssh_session_cache: Arc<Mutex<ssh::credentials::SshCredentialCache>>,
    pub ssh_session_pool: Arc<SshSessionPool>,
}
