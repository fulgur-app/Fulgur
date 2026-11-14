mod monitor;
mod prompter;
mod resolve_user;

use super::remote_types::{
    SSH_USER_PROMPT_TIMEOUT_SECS, format_remote_endpoint_label, wait_for_host_key_decision,
};
use crate::fulgur::{
    Fulgur,
    sync::ssh::{
        self,
        auth::CredentialRequest,
        credentials::SshCredKey,
        pool::{SessionAttempt, retry_once_on_reused_session},
        session::{HostKeyRequest, SshSession},
        url::RemoteSpec,
    },
    ui::notifications::progress::{CancelCallback, start_progress},
};
use monitor::{SshWorkerHandles, spawn_ssh_monitor};
use parking_lot::Mutex;
use prompter::ChannelPrompter;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Connection inputs and presentation labels shared by every remote SSH operation.
pub(super) struct SshTaskContext {
    /// Remote location; `user` must already be resolved.
    pub spec: RemoteSpec,
    /// Verb prefix of the progress label, ending with a space, e.g. `"Connecting to "`.
    pub progress_prefix: &'static str,
    /// Operation named in the logs, e.g. `"open"`.
    pub operation: &'static str,
    /// Label of the message shown when a prompt goes unanswered, e.g. `SSH_CONNECTION_TIMEOUT_LABEL`.
    pub timeout_label: &'static str,
    pub cancel_callback: Option<CancelCallback>,
}

/// Run a remote SSH operation on a worker thread with host-key and credential monitoring.
///
/// ### Arguments
/// - `window`: The window used to show progress and spawn the monitor task
/// - `cx`: The application context
/// - `context`: Connection inputs and labels for this operation
/// - `work`: Operation to run on the established session, given the session, the request
///   spec and the cancel flag, which it checks before changing anything on the remote host.
///   It runs a second time on a new connection when it fails on a session reused from the
///   pool, so it must be safe to repeat.
/// - `publish`: Records an outcome exactly once, guarded by the shared completion flag
/// - `poll_completion`: Returns the payload to deliver once the operation has completed
/// - `on_complete`: Applies the delivered payload on the UI thread
pub(super) fn spawn_ssh_task<Work, Payload>(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<Fulgur>,
    context: SshTaskContext,
    work: impl Fn(&SshSession, &RemoteSpec, &AtomicBool) -> Result<Work, ssh::error::SshError>
    + Send
    + 'static,
    publish: impl Fn(&AtomicBool, Result<Work, String>) -> bool + Send + Sync + 'static,
    poll_completion: impl Fn(&AtomicBool) -> Option<Payload> + 'static,
    on_complete: impl Fn(&mut Fulgur, Payload, &mut gpui_kit::Window, &mut gpui_kit::Context<Fulgur>)
    + 'static,
) where
    Work: Send + 'static,
    Payload: 'static,
{
    let SshTaskContext {
        spec,
        progress_prefix,
        operation,
        timeout_label,
        cancel_callback,
    } = context;
    let ssh_session_cache = Arc::clone(&Fulgur::shared_state(cx).ssh_session_cache);
    let ssh_session_pool = Arc::clone(&Fulgur::shared_state(cx).ssh_session_pool);

    let pending_host_key: Arc<Mutex<Option<HostKeyRequest>>> = Arc::new(Mutex::new(None));
    let pending_host_key_for_thread = Arc::clone(&pending_host_key);
    let pending_credential: Arc<Mutex<Option<CredentialRequest>>> = Arc::new(Mutex::new(None));
    let pending_credential_for_thread = Arc::clone(&pending_credential);
    let finished = Arc::new(AtomicBool::new(false));
    let finished_for_thread = Arc::clone(&finished);
    let prompt_timed_out_for_thread = Arc::new(AtomicBool::new(false));

    let timeout_message_for_thread = format!("{timeout_label} ({SSH_USER_PROMPT_TIMEOUT_SECS} s)");

    let user = spec.user.clone().unwrap_or_default();
    let credential_key = SshCredKey::new(spec.host.clone(), spec.port, user.clone());
    let progress_label =
        format_remote_endpoint_label(progress_prefix, &spec.host, spec.port, &user);
    let log_label = format!(
        "Remote {operation} of {} on {}",
        spec.path,
        format_remote_endpoint_label("", &spec.host, spec.port, &user)
    );
    let log_label_for_thread = log_label.clone();
    let progress = start_progress(window, cx, progress_label.into(), cancel_callback);
    let cancel_flag = progress.cancel_flag();
    let cancel_flag_for_thread = Arc::clone(&cancel_flag);

    let publish = Arc::new(publish);
    let publish_for_thread = Arc::clone(&publish);
    let spec_for_thread = spec;
    let cache_for_thread = ssh_session_cache;
    let pool_for_thread = ssh_session_pool;

    let worker = std::thread::spawn(move || {
        let cached_auth = cache_for_thread.lock().get(&credential_key).cloned();
        let host_key_slot = pending_host_key_for_thread;
        let host_key_timed_out = Arc::clone(&prompt_timed_out_for_thread);
        let request_host_key_decision = move |fingerprint: &str, host: &str, port: u16| {
            let (tx, rx) = std::sync::mpsc::channel();
            *host_key_slot.lock() = Some(HostKeyRequest {
                fingerprint: fingerprint.to_string(),
                host: host.to_string(),
                port,
                decision_tx: tx,
            });
            wait_for_host_key_decision(&rx, &host_key_timed_out)
        };
        let mut prompter = ChannelPrompter {
            pending_request: pending_credential_for_thread,
            host: spec_for_thread.host.clone(),
            port: spec_for_thread.port,
            user: user.clone(),
            timed_out: Arc::clone(&prompt_timed_out_for_thread),
        };

        let outcome = retry_once_on_reused_session(
            || {
                let session_result = pool_for_thread.checkout_or_connect(
                    &spec_for_thread,
                    &user,
                    cached_auth.as_ref(),
                    &request_host_key_decision,
                    &mut prompter,
                );
                match session_result {
                    Ok((pooled_session, auth)) => {
                        cache_for_thread.lock().insert(credential_key.clone(), auth);
                        pooled_session.run(|session| {
                            let result = work(session, &spec_for_thread, &cancel_flag_for_thread);
                            match &result {
                                Ok(_) => log::info!("{log_label_for_thread} succeeded"),
                                Err(ssh::error::SshError::Cancelled) => {}
                                Err(error) => {
                                    log::warn!("{log_label_for_thread} failed: {error}");
                                }
                            }
                            result
                        })
                    }
                    Err(error) => {
                        if matches!(error, ssh::error::SshError::AuthFailed) {
                            cache_for_thread.lock().remove(&credential_key);
                        }
                        SessionAttempt::checkout_failed(error)
                    }
                }
            },
            || {
                !cancel_flag_for_thread.load(Ordering::Acquire)
                    && !prompt_timed_out_for_thread.load(Ordering::Acquire)
            },
        );
        let mut outcome = outcome.map_err(|e| e.user_message());
        if prompt_timed_out_for_thread.load(Ordering::Acquire) {
            log::warn!("{log_label_for_thread} failed: {timeout_message_for_thread}");
            outcome = Err(timeout_message_for_thread);
        }

        if cancel_flag_for_thread.load(Ordering::Acquire) {
            log::info!("{log_label_for_thread} cancelled");
            // User cancelled, discard the outcome and unblock the monitor task.
            finished_for_thread.store(true, Ordering::Release);
        } else {
            (*publish_for_thread)(&finished_for_thread, outcome);
        }
    });

    spawn_ssh_monitor(
        window,
        cx,
        progress,
        SshWorkerHandles {
            worker,
            pending_host_key,
            pending_credential,
            finished,
            cancel_flag,
            log_label,
        },
        publish,
        poll_completion,
        on_complete,
    );
}
