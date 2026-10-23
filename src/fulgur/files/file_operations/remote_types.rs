use crate::fulgur::sync::ssh::{
    save_queue::RemoteSavePermit,
    session::HostKeyDecision,
    sftp::RemoteDirectoryEntry,
    url::{RemoteSpec, format_host_for_url},
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
    /// Remote history URL to record if the requested location opens as asked,
    /// `None` when the open did not come from the Open Remote File dialog.
    pub history_url: Option<String>,
    pub result: Result<RemoteOpenResult, String>,
}

/// Inputs required to execute a remote open in the SSH worker thread.
pub struct RemoteOpenTaskParams {
    pub spec: RemoteSpec,
    pub target_tab_id: Option<TabId>,
    pub target_request_id: Option<u64>,
    pub(crate) target_reload_guard: Option<RemoteReloadGuard>,
    /// Remote history URL to record if the requested location opens as asked,
    /// `None` when the open did not come from the Open Remote File dialog.
    pub history_url: Option<String>,
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
    wait_for_user_answer_within(answer_rx, timed_out, SSH_USER_PROMPT_TIMEOUT)
}

/// Wait at most `timeout` for the user to answer a prompt posted by an SSH worker thread.
///
/// ### Arguments
/// - `answer_rx`: Receiver the prompt dialog delivers its answer on
/// - `timed_out`: Shared flag set when the wait elapsed without an answer
/// - `timeout`: Longest time to wait for the answer
///
/// ### Returns
/// - `Some(T)`: The user's answer
/// - `None`: The dialog closed without answering, or `timeout` elapsed
fn wait_for_user_answer_within<T>(
    answer_rx: &Receiver<T>,
    timed_out: &AtomicBool,
    timeout: Duration,
) -> Option<T> {
    match answer_rx.recv_timeout(timeout) {
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
/// - `host`: Remote host or IP; IPv6 literals are bracketed so the port stays unambiguous.
/// - `port`: SSH port.
/// - `user`: Username; an empty string omits the `user@` prefix.
///
/// ### Returns
/// - `String`: Composed label.
pub fn format_remote_endpoint_label(prefix: &str, host: &str, port: u16, user: &str) -> String {
    let host = format_host_for_url(host);
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

#[cfg(test)]
mod tests {
    use super::{
        format_remote_endpoint_label, wait_for_host_key_decision, wait_for_user_answer,
        wait_for_user_answer_within,
    };
    use crate::fulgur::sync::ssh::session::HostKeyDecision;
    use std::{
        sync::{
            atomic::{AtomicBool, Ordering},
            mpsc,
        },
        time::Duration,
    };

    #[test]
    fn test_wait_for_user_answer_returns_delivered_answer() {
        let (answer_tx, answer_rx) = mpsc::channel();
        let timed_out = AtomicBool::new(false);
        answer_tx.send(42).expect("receiver is alive");

        assert_eq!(wait_for_user_answer(&answer_rx, &timed_out), Some(42));
        assert!(!timed_out.load(Ordering::Acquire));
    }

    #[test]
    fn test_wait_for_user_answer_returns_none_when_sender_dropped() {
        let (answer_tx, answer_rx) = mpsc::channel::<u32>();
        let timed_out = AtomicBool::new(false);
        drop(answer_tx);

        assert_eq!(wait_for_user_answer(&answer_rx, &timed_out), None);
        assert!(
            !timed_out.load(Ordering::Acquire),
            "a closed dialog is not a timeout"
        );
    }

    #[test]
    fn test_wait_for_user_answer_sets_timed_out_flag_when_unanswered() {
        let (_answer_tx, answer_rx) = mpsc::channel::<u32>();
        let timed_out = AtomicBool::new(false);

        let answer = wait_for_user_answer_within(&answer_rx, &timed_out, Duration::from_millis(10));

        assert_eq!(answer, None);
        assert!(timed_out.load(Ordering::Acquire));
    }

    #[test]
    fn test_wait_for_host_key_decision_returns_delivered_decision() {
        let (decision_tx, decision_rx) = mpsc::channel();
        let timed_out = AtomicBool::new(false);
        decision_tx
            .send(HostKeyDecision::Accept)
            .expect("receiver is alive");

        assert!(matches!(
            wait_for_host_key_decision(&decision_rx, &timed_out),
            HostKeyDecision::Accept
        ));
        assert!(!timed_out.load(Ordering::Acquire));
    }

    #[test]
    fn test_wait_for_host_key_decision_rejects_when_sender_dropped() {
        let (decision_tx, decision_rx) = mpsc::channel();
        let timed_out = AtomicBool::new(false);
        drop(decision_tx);

        assert!(matches!(
            wait_for_host_key_decision(&decision_rx, &timed_out),
            HostKeyDecision::Reject
        ));
        assert!(!timed_out.load(Ordering::Acquire));
    }

    #[test]
    fn test_format_remote_endpoint_label_cases() {
        let cases = [
            (
                "Connecting to ",
                "example.com",
                22,
                "alice",
                "Connecting to alice@example.com:22",
            ),
            (
                "Connecting to ",
                "example.com",
                2222,
                "alice",
                "Connecting to alice@example.com:2222",
            ),
            (
                "Saving to ",
                "example.com",
                22,
                "",
                "Saving to example.com:22",
            ),
            ("", "10.0.0.1", 22, "bob", "bob@10.0.0.1:22"),
            (
                "Connecting to ",
                "::1",
                2222,
                "alice",
                "Connecting to alice@[::1]:2222",
            ),
            ("Connecting to ", "[::1]", 22, "", "Connecting to [::1]:22"),
        ];

        for (prefix, host, port, user, expected) in cases {
            assert_eq!(
                format_remote_endpoint_label(prefix, host, port, user),
                expected,
                "prefix={prefix:?} host={host:?} port={port} user={user:?}"
            );
        }
    }
}
