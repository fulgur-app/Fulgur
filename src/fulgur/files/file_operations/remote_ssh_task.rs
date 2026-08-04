use super::remote_types::{
    SSH_HOST_KEY_APPROVAL_TIMEOUT, SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS,
    format_remote_endpoint_label, wait_for_host_key_decision,
};
use crate::fulgur::{
    Fulgur,
    sync::ssh::{
        self,
        credentials::{SshCredKey, SshCredentialCache},
        pool::SshSessionPool,
        session::{HostKeyDecision, HostKeyRequest, SshSession},
        url::RemoteSpec,
    },
    ui::notifications::progress::{CancelCallback, start_progress},
};
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use zeroize::Zeroizing;

/// Interval at which the monitor task checks for completion and host-key prompts.
const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Connection inputs and presentation labels shared by every remote SSH operation.
pub(super) struct SshTaskContext {
    pub spec: RemoteSpec,
    pub password: Zeroizing<String>,
    pub credential_key: SshCredKey,
    pub ssh_session_cache: Arc<Mutex<SshCredentialCache>>,
    pub ssh_session_pool: Arc<SshSessionPool>,
    /// Verb prefix of the progress label, ending with a space, e.g. `"Connecting to "`.
    pub progress_prefix: &'static str,
    /// Label used to build the timeout message, e.g. `SSH_CONNECTION_TIMEOUT_LABEL`.
    pub timeout_label: &'static str,
    pub cancel_callback: Option<CancelCallback>,
}

/// Run a remote SSH operation on a worker thread with host-key and timeout monitoring.
///
/// ### Arguments
/// - `window`: The window used to show progress and spawn the monitor task
/// - `cx`: The application context
/// - `context`: Connection inputs and labels for this operation
/// - `work`: Operation to run on the established session, given the session and the request spec
/// - `publish`: Records an outcome exactly once, guarded by the shared completion flag
/// - `poll_completion`: Returns the payload to deliver once the operation has completed
/// - `on_complete`: Applies the delivered payload on the UI thread
pub(super) fn spawn_ssh_task<Work, Payload>(
    window: &mut gpui::Window,
    cx: &mut gpui::Context<Fulgur>,
    context: SshTaskContext,
    work: impl FnOnce(&SshSession, &RemoteSpec) -> Result<Work, ssh::error::SshError> + Send + 'static,
    publish: impl Fn(&AtomicBool, Result<Work, String>) -> bool + Send + Sync + 'static,
    poll_completion: impl Fn(&AtomicBool) -> Option<Payload> + 'static,
    on_complete: impl Fn(&mut Fulgur, Payload, &mut gpui::Window, &mut gpui::Context<Fulgur>) + 'static,
) where
    Work: Send + 'static,
    Payload: 'static,
{
    let SshTaskContext {
        spec,
        password,
        credential_key,
        ssh_session_cache,
        ssh_session_pool,
        progress_prefix,
        timeout_label,
        cancel_callback,
    } = context;

    let pending_host_key: Arc<Mutex<Option<HostKeyRequest>>> = Arc::new(Mutex::new(None));
    let pending_host_key_for_thread = Arc::clone(&pending_host_key);
    let finished = Arc::new(AtomicBool::new(false));
    let finished_for_thread = Arc::clone(&finished);
    let host_key_decision_timed_out = Arc::new(AtomicBool::new(false));
    let host_key_decision_timed_out_for_thread = Arc::clone(&host_key_decision_timed_out);

    let timeout_message = format!("{timeout_label} ({SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS} s)");
    let timeout_message_for_thread = timeout_message.clone();

    let user = spec.user.clone().unwrap_or_default();
    let progress_label =
        format_remote_endpoint_label(progress_prefix, &spec.host, spec.port, &user);
    let progress = start_progress(window, cx, progress_label.into(), cancel_callback);
    let cancel_flag = progress.cancel_flag();
    let cancel_flag_for_thread = Arc::clone(&cancel_flag);

    let publish = Arc::new(publish);
    let publish_for_thread = Arc::clone(&publish);
    let spec_for_thread = spec;
    let cache_for_thread = ssh_session_cache;
    let pool_for_thread = ssh_session_pool;

    std::thread::spawn(move || {
        let slot = pending_host_key_for_thread;
        let host_key_decision_timed_out_for_callback =
            Arc::clone(&host_key_decision_timed_out_for_thread);
        let session_result = pool_for_thread.checkout_or_connect(
            &spec_for_thread,
            &user,
            &password,
            move |fingerprint, host, port| {
                let (tx, rx) = std::sync::mpsc::channel();
                *slot.lock() = Some(HostKeyRequest {
                    fingerprint: fingerprint.to_string(),
                    host: host.to_string(),
                    port,
                    decision_tx: tx,
                });
                wait_for_host_key_decision(&rx, &host_key_decision_timed_out_for_callback)
            },
        );
        if let Err(ssh::error::SshError::AuthFailed) = &session_result {
            cache_for_thread.lock().remove(&credential_key);
        }

        let mut outcome = session_result
            .and_then(|pooled_session| {
                let result = work(pooled_session.session(), &spec_for_thread);
                if result.is_err() {
                    pooled_session.invalidate();
                }
                result
            })
            .map_err(|e| e.user_message());
        if host_key_decision_timed_out_for_thread.load(Ordering::Acquire) {
            outcome = Err(timeout_message_for_thread);
        }

        if cancel_flag_for_thread.load(Ordering::Acquire) {
            // User cancelled, discard the outcome and unblock the monitor task.
            finished_for_thread.store(true, Ordering::Release);
        } else {
            (*publish_for_thread)(&finished_for_thread, outcome);
        }
    });

    cx.spawn_in(window, async move |view, async_cx| {
        let _progress = progress;
        let deadline = Instant::now() + SSH_HOST_KEY_APPROVAL_TIMEOUT;
        loop {
            async_cx
                .background_executor()
                .timer(MONITOR_POLL_INTERVAL)
                .await;

            let completion = if let Some(payload) = poll_completion(&finished) {
                Some(payload)
            } else if cancel_flag.load(Ordering::Acquire) {
                None
            } else {
                let hk_req = pending_host_key.lock().take();
                if let Some(req) = hk_req {
                    async_cx
                        .update(|window, cx| {
                            _ = view.update(cx, |fulgur, cx| {
                                fulgur.show_ssh_host_fingerprint_dialog(window, cx, req);
                            });
                        })
                        .ok();
                }

                if Instant::now() <= deadline {
                    continue;
                }

                if let Some(request) = pending_host_key.lock().take() {
                    let _ = request.decision_tx.send(HostKeyDecision::Reject);
                }
                (*publish)(&finished, Err(timeout_message.clone()));
                poll_completion(&finished)
            };

            if let Some(payload) = completion {
                async_cx
                    .update(|window, cx| {
                        _ = view.update(cx, |fulgur, cx| {
                            on_complete(fulgur, payload, window, cx);
                        });
                    })
                    .ok();
            }
            break;
        }
    })
    .detach();
}
