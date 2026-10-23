use super::remote_types::{
    SSH_USER_PROMPT_TIMEOUT_SECS, format_remote_endpoint_label, wait_for_host_key_decision,
    wait_for_user_answer,
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
        pool::{SessionAttempt, retry_once_on_reused_session},
        session::{HostKeyRequest, SshHostConfig, SshSession},
        url::RemoteSpec,
    },
    ui::notifications::progress::{CancelCallback, ProgressNotification, start_progress},
};
use parking_lot::Mutex;
use std::{
    path::Path,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread::JoinHandle,
    time::Duration,
};

/// Interval at which the monitor task checks for completion and user prompts.
const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Outcome published when the worker thread ends without publishing one, i.e. it panicked.
const SSH_WORKER_STOPPED_MESSAGE: &str = "SSH worker stopped unexpectedly";

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

/// State the UI monitor task shares with an SSH worker thread.
struct SshWorkerHandles {
    /// The worker thread; once it has finished, a missing outcome means it panicked.
    worker: JoinHandle<()>,
    pending_host_key: Arc<Mutex<Option<HostKeyRequest>>>,
    pending_credential: Arc<Mutex<Option<CredentialRequest>>>,
    /// Completion flag guarding the exactly-once `publish`.
    finished: Arc<AtomicBool>,
    /// Set when the user cancels from the progress notification.
    cancel_flag: Arc<AtomicBool>,
    /// Operation and endpoint named in the logs.
    log_label: String,
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

/// Watch an SSH worker thread until it publishes an outcome, is cancelled, or stops.
///
/// ### Arguments
/// - `window`: The window used to spawn the monitor task and show the prompt dialogs
/// - `cx`: The application context
/// - `progress`: Progress notification, dismissed when the monitor ends
/// - `handles`: State shared with the worker thread
/// - `publish`: Records an outcome exactly once, guarded by the shared completion flag
/// - `poll_completion`: Returns the payload to deliver once the operation has completed
/// - `on_complete`: Applies the delivered payload on the UI thread
fn spawn_ssh_monitor<Work, Payload, Publish>(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<Fulgur>,
    progress: ProgressNotification,
    handles: SshWorkerHandles,
    publish: Arc<Publish>,
    poll_completion: impl Fn(&AtomicBool) -> Option<Payload> + 'static,
    on_complete: impl Fn(&mut Fulgur, Payload, &mut gpui_kit::Window, &mut gpui_kit::Context<Fulgur>)
    + 'static,
) where
    Payload: 'static,
    Publish: Fn(&AtomicBool, Result<Work, String>) -> bool + 'static,
{
    let SshWorkerHandles {
        worker,
        pending_host_key,
        pending_credential,
        finished,
        cancel_flag,
        log_label,
    } = handles;

    cx.spawn_in(window, async move |view, async_cx| {
        let _progress = progress;
        loop {
            async_cx
                .background_executor()
                .timer(MONITOR_POLL_INTERVAL)
                .await;

            let completion = if let Some(payload) = poll_completion(&finished) {
                Some(payload)
            } else if cancel_flag.load(Ordering::Acquire) {
                None
            } else if worker.is_finished() {
                // A worker that returns always publishes, so it panicked. The exactly-once
                // guard keeps an outcome it published after the poll above.
                if (*publish)(&finished, Err(SSH_WORKER_STOPPED_MESSAGE.to_string())) {
                    log::error!("{log_label} failed: {SSH_WORKER_STOPPED_MESSAGE}");
                }
                poll_completion(&finished)
            } else {
                let host_key_request = pending_host_key.lock().take();
                let credential_request = pending_credential.lock().take();
                if host_key_request.is_some() || credential_request.is_some() {
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
                continue;
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

#[cfg(test)]
mod tests {
    use super::ChannelPrompter;
    use crate::fulgur::sync::ssh::auth::{
        CredentialPrompter, CredentialRequest, CredentialRequestKind, LoginAnswer,
    };
    use parking_lot::Mutex;
    use std::{
        sync::{Arc, atomic::AtomicBool},
        thread,
        time::{Duration, Instant},
    };
    use zeroize::Zeroizing;

    /// Longest time a test waits for the worker thread to post its prompt.
    const POSTED_REQUEST_DEADLINE: Duration = Duration::from_secs(5);

    /// Build a prompter for `alice@example.com:2222` and the slot it posts requests to.
    ///
    /// ### Returns
    /// - `(ChannelPrompter, Arc<Mutex<Option<CredentialRequest>>>)`: The prompter and its
    ///   pending request slot, as read by the monitor task
    fn make_prompter() -> (ChannelPrompter, Arc<Mutex<Option<CredentialRequest>>>) {
        let pending_request = Arc::new(Mutex::new(None));
        let prompter = ChannelPrompter {
            pending_request: Arc::clone(&pending_request),
            host: "example.com".to_string(),
            port: 2222,
            user: "alice".to_string(),
            timed_out: Arc::new(AtomicBool::new(false)),
        };
        (prompter, pending_request)
    }

    /// Wait for the worker thread to post a prompt, then take it as the monitor task does.
    ///
    /// ### Arguments
    /// - `pending_request`: Slot the prompter posts its request to
    ///
    /// ### Returns
    /// - `CredentialRequest`: The posted request; panics after `POSTED_REQUEST_DEADLINE`
    fn take_posted_request(
        pending_request: &Mutex<Option<CredentialRequest>>,
    ) -> CredentialRequest {
        let deadline = Instant::now() + POSTED_REQUEST_DEADLINE;
        loop {
            if let Some(request) = pending_request.lock().take() {
                return request;
            }
            assert!(
                Instant::now() < deadline,
                "the prompter never posted a request"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn test_channel_prompter_login_forwards_answer_from_ui_thread() {
        let (mut prompter, pending_request) = make_prompter();
        let worker = thread::spawn(move || prompter.login(true, Some("denied".to_string())));

        let request = take_posted_request(&pending_request);
        assert_eq!(request.host, "example.com");
        assert_eq!(request.port, 2222);
        assert_eq!(request.user, "alice");
        let CredentialRequestKind::Login {
            password_allowed,
            error,
            answer_tx,
        } = request.kind
        else {
            panic!("expected a login request");
        };
        assert!(password_allowed);
        assert_eq!(error.as_deref(), Some("denied"));
        answer_tx
            .send(LoginAnswer::Password(Zeroizing::new("secret".to_string())))
            .expect("worker is waiting for the answer");

        let answer = worker.join().expect("worker thread panicked");
        assert!(matches!(answer, LoginAnswer::Password(password) if password.as_str() == "secret"));
    }

    #[test]
    fn test_channel_prompter_login_cancels_when_monitor_drops_request() {
        let (mut prompter, pending_request) = make_prompter();
        let worker = thread::spawn(move || prompter.login(true, None));

        // On timeout the monitor task drops the unanswered request, closing its channel.
        drop(take_posted_request(&pending_request));

        let answer = worker.join().expect("worker thread panicked");
        assert!(matches!(answer, LoginAnswer::Cancel));
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_with_resolved_remote_user_moves_url_password_to_cache(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        use crate::fulgur::{
            Fulgur,
            files::file_operations::test_helpers::setup_fulgur,
            sync::ssh::{
                auth::SshAuth,
                credentials::SshCredKey,
                url::{RemoteSpec, parse_remote_url},
            },
        };
        use std::{cell::RefCell, rc::Rc};

        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let spec = parse_remote_url("ssh://alice:secret@example.com:2222/tmp/notes.txt")
            .expect("valid remote URL");
        assert!(spec.password_in_url.is_some());
        let resolved: Rc<RefCell<Option<RemoteSpec>>> = Rc::new(RefCell::new(None));
        let resolved_in_callback = Rc::clone(&resolved);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.with_resolved_remote_user(window, cx, spec, move |_, spec, _, _| {
                    *resolved_in_callback.borrow_mut() = Some(spec);
                });
            });
        });

        let resolved = resolved
            .take()
            .expect("a URL with a user resolves without the username dialog");
        assert_eq!(resolved.user.as_deref(), Some("alice"));
        assert!(
            resolved.password_in_url.is_none(),
            "the password must not travel further in the spec"
        );
        visual_cx.update(|_, cx| {
            let cache = Fulgur::shared_state(cx).ssh_session_cache.lock();
            let cached = cache.get(&SshCredKey::new("example.com", 2222, "alice"));
            assert!(
                matches!(cached, Some(SshAuth::Password(password)) if password.as_str() == "secret")
            );
        });
    }

    #[cfg(feature = "gpui-test-support")]
    mod gpui_monitor_tests {
        use super::super::{
            MONITOR_POLL_INTERVAL, SSH_WORKER_STOPPED_MESSAGE, SshWorkerHandles, spawn_ssh_monitor,
        };
        use crate::fulgur::{
            Fulgur,
            files::file_operations::{
                remote_types::SSH_USER_PROMPT_TIMEOUT, test_helpers::setup_fulgur_with_root,
            },
            ui::notifications::progress::start_progress,
        };
        use gpui_kit::{Entity, VisualTestContext};
        use parking_lot::Mutex;
        use std::{
            cell::RefCell,
            rc::Rc,
            sync::{Arc, atomic::AtomicBool, mpsc},
            thread::{self, JoinHandle},
            time::{Duration, Instant},
        };

        /// Save outcome slot shared by the worker's `publish` and the monitor's poll.
        type PendingSave = Arc<Mutex<Option<Result<(), String>>>>;

        /// Longest time a test waits on its worker thread.
        const WORKER_DEADLINE: Duration = Duration::from_secs(5);

        /// Watch `worker` with a save monitor and record the outcome it delivers.
        ///
        /// ### Arguments
        /// - `visual_cx`: The visual test context driving the window
        /// - `fulgur`: The window's Fulgur entity
        /// - `worker`: The worker thread to watch
        /// - `finished`: Completion flag shared with the worker
        /// - `pending_save`: Outcome slot shared with the worker
        ///
        /// ### Returns
        /// - `Rc<RefCell<Option<Result<(), String>>>>`: Filled with the outcome once the
        ///   monitor delivers it on the UI thread
        fn start_save_monitor(
            visual_cx: &mut VisualTestContext,
            fulgur: &Entity<Fulgur>,
            worker: JoinHandle<()>,
            finished: Arc<AtomicBool>,
            pending_save: PendingSave,
        ) -> Rc<RefCell<Option<Result<(), String>>>> {
            let delivered = Rc::new(RefCell::new(None));
            let delivered_by_monitor = Rc::clone(&delivered);
            let pending_save_for_publish = Arc::clone(&pending_save);
            visual_cx.update(|window, cx| {
                fulgur.update(cx, |_, cx| {
                    let progress = start_progress(window, cx, "Saving".into(), None);
                    let cancel_flag = progress.cancel_flag();
                    spawn_ssh_monitor(
                        window,
                        cx,
                        progress,
                        SshWorkerHandles {
                            worker,
                            pending_host_key: Arc::default(),
                            pending_credential: Arc::default(),
                            finished,
                            cancel_flag,
                            log_label: "Remote save of /tmp/notes.txt".to_string(),
                        },
                        Arc::new(move |finished: &AtomicBool, outcome: Result<(), String>| {
                            Fulgur::publish_remote_save_outcome(
                                finished,
                                &pending_save_for_publish,
                                outcome,
                            )
                        }),
                        move |_| pending_save.lock().take(),
                        move |_, result, _, _| {
                            *delivered_by_monitor.borrow_mut() = Some(result);
                        },
                    );
                });
            });
            delivered
        }

        #[gpui_kit::test]
        fn test_ssh_monitor_waits_for_worker_past_prompt_timeout(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
            let finished = Arc::new(AtomicBool::new(false));
            let pending_save = PendingSave::default();
            let (release_tx, release_rx) = mpsc::channel::<()>();
            let (published_tx, published_rx) = mpsc::channel::<()>();
            let worker = {
                let finished = Arc::clone(&finished);
                let pending_save = Arc::clone(&pending_save);
                thread::spawn(move || {
                    release_rx.recv().ok();
                    Fulgur::publish_remote_save_outcome(&finished, &pending_save, Ok(()));
                    published_tx.send(()).ok();
                })
            };
            let delivered = start_save_monitor(
                &mut visual_cx,
                &fulgur,
                worker,
                Arc::clone(&finished),
                pending_save,
            );

            visual_cx
                .background_executor
                .advance_clock(SSH_USER_PROMPT_TIMEOUT * 2);
            visual_cx.run_until_parked();
            assert!(
                delivered.borrow().is_none(),
                "a worker still transferring must not be reported as timed out"
            );

            release_tx
                .send(())
                .expect("worker is waiting to be released");
            published_rx
                .recv_timeout(WORKER_DEADLINE)
                .expect("worker published its outcome");
            visual_cx
                .background_executor
                .advance_clock(MONITOR_POLL_INTERVAL);
            visual_cx.run_until_parked();
            assert_eq!(*delivered.borrow(), Some(Ok(())));
        }

        #[gpui_kit::test]
        fn test_ssh_monitor_reports_worker_that_stopped_without_outcome(
            cx: &mut gpui_kit::TestAppContext,
        ) {
            let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
            // Ends without publishing, as a panicking worker does.
            let worker = thread::spawn(|| {});
            let deadline = Instant::now() + WORKER_DEADLINE;
            while !worker.is_finished() {
                assert!(Instant::now() < deadline, "the worker thread never ended");
                thread::sleep(Duration::from_millis(1));
            }
            let delivered = start_save_monitor(
                &mut visual_cx,
                &fulgur,
                worker,
                Arc::new(AtomicBool::new(false)),
                PendingSave::default(),
            );

            visual_cx
                .background_executor
                .advance_clock(MONITOR_POLL_INTERVAL);
            visual_cx.run_until_parked();
            assert_eq!(
                *delivered.borrow(),
                Some(Err(SSH_WORKER_STOPPED_MESSAGE.to_string()))
            );
        }
    }
}
