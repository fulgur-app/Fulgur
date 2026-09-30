use crate::fulgur::sync::ssh::{
    save_queue::RemoteSavePermit, session::HostKeyDecision, sftp::RemoteDirectoryEntry,
    url::RemoteSpec,
};
use crate::fulgur::ui::tabs::editor_tab::ContentRevision;
use crate::fulgur::ui::tabs::tab::TabId;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, RecvTimeoutError},
    },
    time::Duration,
};

/// Time the user has to answer a host-key or credential prompt, and the budget of the
/// remote operation after the last prompt.
pub const SSH_USER_PROMPT_TIMEOUT_SECS: u64 = 60;
pub const SSH_USER_PROMPT_TIMEOUT: Duration = Duration::from_secs(SSH_USER_PROMPT_TIMEOUT_SECS);
pub const SSH_CONNECTION_TIMEOUT_LABEL: &str = "SSH connection timed out";
pub const SSH_SAVE_TIMEOUT_LABEL: &str = "SSH save timed out";

/// Result of a successfully loaded remote file, delivered by the SSH background thread.
pub struct RemoteFileResult {
    pub spec: RemoteSpec,
    pub content: String,
    pub encoding: String,
    pub lossy: bool,
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
    /// The remote file looks binary and was not loaded.
    Binary(RemoteSpec),
}

/// Existing-tab state that must remain unchanged during a remote reload.
#[derive(Clone)]
pub(crate) struct RemoteReloadGuard {
    pub content_revision: ContentRevision,
    pub source_url: String,
}

/// A queued remote-open outcome consumed by `Fulgur::process_pending_remote_files`.
pub struct PendingRemoteOpenOutcome {
    pub target_tab_id: Option<TabId>,
    pub target_request_id: Option<u64>,
    pub(crate) target_reload_guard: Option<RemoteReloadGuard>,
    pub result: Result<RemoteOpenResult, String>,
}

/// Inputs required to execute a remote open in the SSH worker thread.
pub struct RemoteOpenTaskParams {
    pub spec: RemoteSpec,
    pub target_tab_id: Option<TabId>,
    pub target_request_id: Option<u64>,
    pub(crate) target_reload_guard: Option<RemoteReloadGuard>,
}

/// Wait for the user to answer a prompt posted by an SSH worker thread.
///
/// ### Arguments
/// - `answer_rx`: Receiver the prompt dialog delivers its answer on
/// - `timed_out`: Shared flag set when the wait elapsed without an answer
///
/// ### Returns
/// - `Some(T)`: The user's answer
/// - `None`: The dialog closed without answering, or `SSH_USER_PROMPT_TIMEOUT` elapsed
pub fn wait_for_user_answer<T>(answer_rx: &Receiver<T>, timed_out: &AtomicBool) -> Option<T> {
    match answer_rx.recv_timeout(SSH_USER_PROMPT_TIMEOUT) {
        Ok(answer) => Some(answer),
        Err(RecvTimeoutError::Timeout) => {
            timed_out.store(true, Ordering::Release);
            None
        }
        Err(RecvTimeoutError::Disconnected) => None,
    }
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
    decision_rx: &Receiver<HostKeyDecision>,
    timed_out: &AtomicBool,
) -> HostKeyDecision {
    wait_for_user_answer(decision_rx, timed_out).unwrap_or(HostKeyDecision::Reject)
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
    pub tab_id: TabId,
    pub request_id: u64,
    pub spec: RemoteSpec,
    pub saved_content: Arc<String>,
    pub saved_bytes: Arc<Vec<u8>>,
    pub remote_save_permit: RemoteSavePermit,
}
