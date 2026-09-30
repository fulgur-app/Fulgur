use super::remote_types::{
    SSH_USER_PROMPT_TIMEOUT, SSH_USER_PROMPT_TIMEOUT_SECS, format_remote_endpoint_label,
    wait_for_host_key_decision, wait_for_user_answer,
};
use crate::fulgur::{
    Fulgur,
    sync::ssh::{
        self,
        auth::{
            ChallengeAnswer, CredentialPrompter, CredentialRequest, CredentialRequestKind,
            LoginAnswer, PassphraseAnswer, SshAuth,
        },
        credentials::SshCredKey,
        session::{HostKeyDecision, HostKeyRequest, SshHostConfig, SshSession},
        url::RemoteSpec,
    },
    ui::notifications::progress::{CancelCallback, start_progress},
};
use parking_lot::Mutex;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    time::{Duration, Instant},
};

/// Interval at which the monitor task checks for completion and user prompts.
const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Connection inputs and presentation labels shared by every remote SSH operation.
pub(super) struct SshTaskContext {
    /// Remote location; `user` must already be resolved.
    pub spec: RemoteSpec,
    /// Verb prefix of the progress label, ending with a space, e.g. `"Connecting to "`.
    pub progress_prefix: &'static str,
    /// Label used to build the timeout message, e.g. `SSH_CONNECTION_TIMEOUT_LABEL`.
    pub timeout_label: &'static str,
    pub cancel_callback: Option<CancelCallback>,
}

/// Credential prompter that hands prompts to the UI monitor task and blocks until answered.
struct ChannelPrompter {
    pending_request: Arc<Mutex<Option<CredentialRequest>>>,
    host: String,
    port: u16,
    user: String,
    timed_out: Arc<AtomicBool>,
}

impl ChannelPrompter {
    /// Post a prompt for the monitor task and wait for the user's answer.
    ///
    /// ### Arguments
    /// - `kind`: Builds the prompt around the channel the answer is sent on.
    ///
    /// ### Returns
    /// - `Some(T)`: The user's answer.
    /// - `None`: The dialog closed without answering, or the prompt timed out.
    fn ask<T>(&self, kind: impl FnOnce(Sender<T>) -> CredentialRequestKind) -> Option<T> {
        let (answer_tx, answer_rx) = std::sync::mpsc::channel();
        *self.pending_request.lock() = Some(CredentialRequest {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            kind: kind(answer_tx),
        });
        wait_for_user_answer(&answer_rx, &self.timed_out)
    }
}

impl CredentialPrompter for ChannelPrompter {
    fn passphrase(&mut self, key_path: &Path, error: Option<String>) -> PassphraseAnswer {
        self.ask(|answer_tx| CredentialRequestKind::Passphrase {
            key_path: key_path.to_path_buf(),
            error,
            answer_tx,
        })
        .unwrap_or(PassphraseAnswer::Cancel)
    }

    fn login(&mut self, password_allowed: bool, error: Option<String>) -> LoginAnswer {
        self.ask(|answer_tx| CredentialRequestKind::Login {
            password_allowed,
            error,
            answer_tx,
        })
        .unwrap_or(LoginAnswer::Cancel)
    }

    fn challenge(&mut self, instructions: &str, prompt: &str, echo: bool) -> ChallengeAnswer {
        self.ask(|answer_tx| CredentialRequestKind::Challenge {
            instructions: instructions.to_string(),
            prompt: prompt.to_string(),
            echo,
            answer_tx,
        })
        .unwrap_or(ChallengeAnswer::Cancel)
    }
}

impl Fulgur {
    /// Prepare the credentials of a remote request, then continue once the user is known.
    ///
    /// A password embedded in the URL moves into the session credential cache. When the
    /// URL has no user, the `User` from `~/.ssh/config` applies, and the username dialog is
    /// shown only when none is configured.
    ///
    /// ### Arguments
    /// - `window`: The window used to show the username dialog
    /// - `cx`: The application context
    /// - `spec`: Remote location, possibly without user and with a URL password
    /// - `on_resolved`: Continuation receiving the spec with its user set and no password
    pub(super) fn with_resolved_remote_user(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        mut spec: RemoteSpec,
        on_resolved: impl Fn(
            &mut Fulgur,
            RemoteSpec,
            &mut gpui_kit::Window,
            &mut gpui_kit::Context<Fulgur>,
        ) + 'static,
    ) {
        if spec.user.is_none() {
            spec.user = SshHostConfig::load(&spec.host).user;
        }
        let url_password = spec.password_in_url.take();
        if let (Some(user), Some(password)) = (spec.user.clone(), url_password) {
            let key = SshCredKey::new(spec.host.clone(), spec.port, user);
            Fulgur::shared_state(cx)
                .ssh_session_cache
                .lock()
                .insert(key, SshAuth::Password(password));
        }
        if spec.user.is_some() {
            on_resolved(self, spec, window, cx);
            return;
        }

        let entity = cx.entity().downgrade();
        let host = spec.host.clone();
        let port = spec.port;
        self.show_ssh_username_dialog(window, cx, &host, port, move |user, window, cx| {
            let mut spec_with_user = spec.clone();
            spec_with_user.user = Some(user);
            if let Some(entity) = entity.upgrade() {
                entity.update(cx, |fulgur, cx| {
                    on_resolved(fulgur, spec_with_user, window, cx);
                });
            }
        });
    }
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
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<Fulgur>,
    context: SshTaskContext,
    work: impl FnOnce(&SshSession, &RemoteSpec) -> Result<Work, ssh::error::SshError> + Send + 'static,
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
    let prompt_timed_out = Arc::new(AtomicBool::new(false));
    let prompt_timed_out_for_thread = Arc::clone(&prompt_timed_out);

    let timeout_message = format!("{timeout_label} ({SSH_USER_PROMPT_TIMEOUT_SECS} s)");
    let timeout_message_for_thread = timeout_message.clone();

    let user = spec.user.clone().unwrap_or_default();
    let credential_key = SshCredKey::new(spec.host.clone(), spec.port, user.clone());
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
        let cached_auth = cache_for_thread.lock().get(&credential_key).cloned();
        let host_key_slot = pending_host_key_for_thread;
        let host_key_timed_out = Arc::clone(&prompt_timed_out_for_thread);
        let mut prompter = ChannelPrompter {
            pending_request: pending_credential_for_thread,
            host: spec_for_thread.host.clone(),
            port: spec_for_thread.port,
            user: user.clone(),
            timed_out: Arc::clone(&prompt_timed_out_for_thread),
        };
        let session_result = pool_for_thread.checkout_or_connect(
            &spec_for_thread,
            &user,
            cached_auth.as_ref(),
            move |fingerprint, host, port| {
                let (tx, rx) = std::sync::mpsc::channel();
                *host_key_slot.lock() = Some(HostKeyRequest {
                    fingerprint: fingerprint.to_string(),
                    host: host.to_string(),
                    port,
                    decision_tx: tx,
                });
                wait_for_host_key_decision(&rx, &host_key_timed_out)
            },
            &mut prompter,
        );
        match &session_result {
            Ok((_, auth)) => {
                cache_for_thread
                    .lock()
                    .insert(credential_key.clone(), auth.clone());
            }
            Err(ssh::error::SshError::AuthFailed) => {
                cache_for_thread.lock().remove(&credential_key);
            }
            Err(_) => {}
        }

        let mut outcome = session_result
            .and_then(|(pooled_session, _)| {
                let result = work(pooled_session.session(), &spec_for_thread);
                if result.is_err() {
                    pooled_session.invalidate();
                }
                result
            })
            .map_err(|e| e.user_message());
        if prompt_timed_out_for_thread.load(Ordering::Acquire) {
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
        let mut deadline = Instant::now() + SSH_USER_PROMPT_TIMEOUT;
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
                let host_key_request = pending_host_key.lock().take();
                let credential_request = pending_credential.lock().take();
                if host_key_request.is_some() || credential_request.is_some() {
                    // The user gets a full timeout to answer each prompt.
                    deadline = Instant::now() + SSH_USER_PROMPT_TIMEOUT;
                    async_cx
                        .update(|window, cx| {
                            _ = view.update(cx, |fulgur, cx| {
                                if let Some(request) = host_key_request {
                                    fulgur.show_ssh_host_fingerprint_dialog(window, cx, request);
                                }
                                if let Some(request) = credential_request {
                                    fulgur.show_ssh_credential_dialog(window, cx, request);
                                }
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
                // Dropping an unanswered credential request unblocks the worker.
                pending_credential.lock().take();
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
