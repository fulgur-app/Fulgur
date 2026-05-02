use super::{
    encoding::detect_encoding_and_decode,
    remote_types::{
        PendingRemoteOpenOutcome, RemoteBrowseResult, RemoteFileResult, RemoteOpenResult,
        RemoteOpenTaskParams, SSH_CONNECTION_TIMEOUT_LABEL, SSH_HOST_KEY_APPROVAL_TIMEOUT,
        SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS, format_remote_endpoint_label,
        wait_for_host_key_decision,
    },
};
use crate::fulgur::{
    Fulgur,
    sync::ssh::{
        self,
        credentials::SshCredKey,
        session::{HostKeyDecision, HostKeyRequest},
        url::{RemoteSpec, format_remote_url},
    },
    ui::notifications::progress::{CancelCallback, start_progress},
};
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

impl Fulgur {
    /// Open a remote file from a parsed `RemoteSpec`.
    ///
    /// ### Arguments
    /// - `window`: The window to open the tab in
    /// - `cx`: The application context
    /// - `spec`: The parsed remote file specification
    pub fn do_open_remote_file(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        spec: RemoteSpec,
    ) {
        if let Some(tab_index) = self.find_tab_by_remote_spec(&spec) {
            self.active_tab_index = Some(tab_index);
            self.focus_active_tab(window, cx);
            cx.notify();
            return;
        }
        self.last_failed_remote_open_url = Some(format_remote_url(&spec));
        self.open_remote_file_with_target(window, cx, spec, None);
    }

    /// Start loading a remote file and apply the result either to a new tab or an existing tab.
    ///
    /// ### Arguments
    /// - `window`: The target window
    /// - `cx`: The application context
    /// - `spec`: The remote specification
    /// - `target_tab_id`: Existing tab to refresh, or `None` to open in a new tab
    fn open_remote_file_with_target(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        mut spec: RemoteSpec,
        target_tab_id: Option<usize>,
    ) {
        let ssh_session_cache = Arc::clone(&self.shared_state(cx).ssh_session_cache);
        let ssh_session_pool = Arc::clone(&self.shared_state(cx).ssh_session_pool);
        let target_request_id = target_tab_id.map(|tab_id| {
            let request_id = self.next_remote_request_id;
            self.next_remote_request_id = self.next_remote_request_id.wrapping_add(1);
            self.latest_remote_open_request_by_tab
                .insert(tab_id, request_id);
            request_id
        });

        // If the URL embeds a password, move it immediately into the session cache.
        if let (Some(user), Some(password)) = (spec.user.clone(), spec.password_in_url.take()) {
            let key = SshCredKey::new(spec.host.clone(), spec.port, user);
            ssh_session_cache.lock().insert(key, password);
        }

        // Reuse a cached password when we already know the user.
        if let Some(user) = spec.user.clone() {
            let cache_key = SshCredKey::new(spec.host.clone(), spec.port, user.clone());
            if let Some(cached_password) = ssh_session_cache.lock().get(&cache_key).cloned() {
                spec.password_in_url = None;
                self.spawn_ssh_open_task(
                    window,
                    cx,
                    RemoteOpenTaskParams {
                        spec,
                        password: cached_password,
                        credential_key: cache_key,
                        ssh_session_cache: Arc::clone(&ssh_session_cache),
                        ssh_session_pool: Arc::clone(&ssh_session_pool),
                        target_tab_id,
                        target_request_id,
                    },
                );
                return;
            }
        }

        let host = spec.host.clone();
        let port = spec.port;
        let user = spec.user.clone();
        let entity = cx.entity().downgrade();
        let cache_for_callback = Arc::clone(&ssh_session_cache);
        let pool_for_callback = Arc::clone(&ssh_session_pool);

        self.show_ssh_password_dialog(
            window,
            cx,
            &host,
            port,
            user,
            move |resolved_user, password, window, cx| {
                let mut spec_with_user = spec.clone();
                spec_with_user.user = Some(resolved_user);
                spec_with_user.password_in_url = None;
                let cache_key = SshCredKey::new(
                    spec_with_user.host.clone(),
                    spec_with_user.port,
                    spec_with_user.user.clone().unwrap_or_default(),
                );
                if let Some(entity) = entity.upgrade() {
                    entity.update(cx, |fulgur, cx| {
                        cache_for_callback
                            .lock()
                            .insert(cache_key.clone(), password.clone());
                        fulgur.spawn_ssh_open_task(
                            window,
                            cx,
                            RemoteOpenTaskParams {
                                spec: spec_with_user,
                                password,
                                credential_key: cache_key,
                                ssh_session_cache: Arc::clone(&cache_for_callback),
                                ssh_session_pool: Arc::clone(&pool_for_callback),
                                target_tab_id,
                                target_request_id,
                            },
                        );
                    });
                }
            },
        );
    }

    /// Lazily reconnect and reload a restored remote tab when the user activates it.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `tab_id`: Stable tab id to refresh
    /// - `spec`: Remote location spec for the tab
    pub fn ensure_remote_tab_loaded(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        tab_id: usize,
        spec: RemoteSpec,
    ) {
        self.open_remote_file_with_target(window, cx, spec, Some(tab_id));
    }

    /// Resolve a remote open request into either file contents or a browse fallback.
    ///
    /// ### Arguments
    /// - `session`: Established SSH session with SFTP subsystem.
    /// - `spec`: Requested remote location.
    ///
    /// ### Returns
    /// - `Ok(RemoteOpenResult::File)`: Target is a readable file.
    /// - `Ok(RemoteOpenResult::Browse)`: Target is a directory or missing path.
    /// - `Err(SshError)`: Remote classification or I/O failure.
    fn resolve_remote_open_result(
        session: &ssh::session::SshSession,
        spec: &RemoteSpec,
    ) -> Result<RemoteOpenResult, ssh::error::SshError> {
        use ssh::sftp::{RemotePathKind, classify_remote_path, closest_existing_remote_directory};

        match classify_remote_path(session, &spec.path)? {
            RemotePathKind::File => {
                let bytes = ssh::sftp::read_remote_file(session, &spec.path)?;
                let (encoding, content) = detect_encoding_and_decode(&bytes);
                Ok(RemoteOpenResult::File(RemoteFileResult {
                    spec: spec.clone(),
                    content,
                    encoding,
                    file_size: bytes.len(),
                }))
            }
            RemotePathKind::Directory => {
                Self::build_remote_browse_result(session, spec, &spec.path, None)
            }
            RemotePathKind::Missing => {
                let fallback_directory = closest_existing_remote_directory(session, &spec.path)?;
                let notice = Some(format!(
                    "Remote path '{}' was not found. Showing closest existing directory '{}'.",
                    spec.path, fallback_directory
                ));
                Self::build_remote_browse_result(session, spec, &fallback_directory, notice)
            }
        }
    }

    /// Build remote browser entries for a directory fallback dialog.
    ///
    /// ### Arguments
    /// - `session`: Established SSH session with SFTP subsystem.
    /// - `base_spec`: Original open request spec.
    /// - `directory_path`: Directory path to list.
    /// - `notice`: Optional informational message for the UI.
    ///
    /// ### Returns
    /// - `Ok(RemoteOpenResult::Browse)`: Browser payload with directory entries.
    /// - `Err(SshError)`: Directory listing failed.
    fn build_remote_browse_result(
        session: &ssh::session::SshSession,
        base_spec: &RemoteSpec,
        directory_path: &str,
        notice: Option<String>,
    ) -> Result<RemoteOpenResult, ssh::error::SshError> {
        let directory = if directory_path.trim().is_empty() {
            ssh::REMOTE_ROOT_PATH.to_string()
        } else {
            directory_path.to_string()
        };
        let mut directory_spec = base_spec.clone();
        directory_spec.path = directory.clone();

        let mut entries: Vec<ssh::sftp::RemoteDirectoryEntry> = Vec::new();
        if directory != ssh::REMOTE_ROOT_PATH {
            let parent = ssh::sftp::parent_remote_path(&directory);
            entries.push(ssh::sftp::RemoteDirectoryEntry {
                name: "..".to_string(),
                is_dir: true,
                full_path: parent,
            });
        }
        entries.extend(ssh::sftp::list_remote_directory(session, &directory)?);

        Ok(RemoteOpenResult::Browse(RemoteBrowseResult {
            directory_spec,
            entries,
            notice,
        }))
    }

    /// Spawn the blocking SSH connect + file-read task plus a GPUI monitoring task.
    ///
    /// ### Arguments
    /// - `window`: The window context used to spawn the async monitoring task
    /// - `cx`: The application context
    /// - `params`: All data required to run the remote open operation
    fn spawn_ssh_open_task(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        params: RemoteOpenTaskParams,
    ) {
        let RemoteOpenTaskParams {
            spec,
            password,
            credential_key,
            ssh_session_cache,
            ssh_session_pool,
            target_tab_id,
            target_request_id,
        } = params;
        if let Some(tab_id) = target_tab_id {
            self.inflight_remote_restore.insert(tab_id);
        }
        let pending_remote_open = Arc::clone(&self.pending_remote_open);

        let pending_host_key: Arc<Mutex<Option<HostKeyRequest>>> = Arc::new(Mutex::new(None));
        let pending_host_key_for_thread = Arc::clone(&pending_host_key);
        let pending_host_key_for_task = Arc::clone(&pending_host_key);

        let pending_remote_for_thread = Arc::clone(&pending_remote_open);
        let pending_remote_for_task = Arc::clone(&pending_remote_open);
        let open_finished = Arc::new(AtomicBool::new(false));
        let open_finished_for_thread = Arc::clone(&open_finished);
        let open_finished_for_task = Arc::clone(&open_finished);
        let host_key_decision_timed_out = Arc::new(AtomicBool::new(false));
        let host_key_decision_timed_out_for_thread = Arc::clone(&host_key_decision_timed_out);

        let spec_for_thread = spec.clone();
        let user = spec.user.clone().unwrap_or_default();
        let cache_for_thread = Arc::clone(&ssh_session_cache);
        let credential_key_for_thread = credential_key.clone();
        let pool_for_thread = Arc::clone(&ssh_session_pool);

        let progress_label =
            format_remote_endpoint_label("Connecting to ", &spec.host, spec.port, &user);
        let entity_weak = cx.entity().downgrade();
        let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, cx| {
            if let Some(entity) = entity_weak.upgrade() {
                entity.update(cx, |fulgur, _cx| {
                    if let Some(tab_id) = target_tab_id {
                        fulgur.latest_remote_open_request_by_tab.remove(&tab_id);
                        fulgur.inflight_remote_restore.remove(&tab_id);
                    }
                });
            }
        }));
        let progress = start_progress(window, cx, progress_label.into(), cancel_callback);
        let cancel_flag = progress.cancel_flag();
        let cancel_flag_for_thread = Arc::clone(&cancel_flag);

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
                cache_for_thread.lock().remove(&credential_key_for_thread);
            }

            let mut outcome = session_result
                .and_then(|pooled_session| {
                    let result = if target_tab_id.is_some() {
                        ssh::sftp::read_remote_file(pooled_session.session(), &spec_for_thread.path)
                            .map(|bytes| {
                                let (encoding, content) = detect_encoding_and_decode(&bytes);
                                RemoteOpenResult::File(RemoteFileResult {
                                    spec: spec_for_thread.clone(),
                                    content,
                                    encoding,
                                    file_size: bytes.len(),
                                })
                            })
                    } else {
                        Self::resolve_remote_open_result(pooled_session.session(), &spec_for_thread)
                    };
                    if result.is_err() {
                        pooled_session.invalidate();
                    }
                    result
                })
                .map_err(|e| e.user_message());
            if host_key_decision_timed_out_for_thread.load(Ordering::Acquire) {
                outcome = Err(format!(
                    "{SSH_CONNECTION_TIMEOUT_LABEL} ({SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS} s)"
                ));
            }

            if cancel_flag_for_thread.load(Ordering::Acquire) {
                // User cancelled, discard the outcome and unblock the monitor task.
                open_finished_for_thread.store(true, Ordering::Release);
            } else {
                Self::publish_remote_open_outcome(
                    &open_finished_for_thread,
                    &pending_remote_for_thread,
                    target_tab_id,
                    target_request_id,
                    outcome,
                );
            }
        });

        cx.spawn_in(window, async move |view, async_cx| {
            let _progress = progress;
            let deadline = std::time::Instant::now() + SSH_HOST_KEY_APPROVAL_TIMEOUT;
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

                if open_finished_for_task.load(Ordering::Acquire) {
                    async_cx
                        .update(|_, cx| {
                            _ = view.update(cx, |_, cx| cx.notify());
                        })
                        .ok();
                    break;
                }

                let hk_req = pending_host_key_for_task.lock().take();
                if let Some(req) = hk_req {
                    async_cx
                        .update(|window, cx| {
                            _ = view.update(cx, |fulgur, cx| {
                                fulgur.show_ssh_host_fingerprint_dialog(window, cx, req);
                            });
                        })
                        .ok();
                }

                if std::time::Instant::now() > deadline {
                    if let Some(request) = pending_host_key_for_task.lock().take() {
                        let _ = request.decision_tx.send(HostKeyDecision::Reject);
                    }
                    if cancel_flag.load(Ordering::Acquire) {
                        open_finished_for_task.store(true, Ordering::Release);
                    } else {
                        Self::publish_remote_open_outcome(
                            &open_finished_for_task,
                            &pending_remote_for_task,
                            target_tab_id,
                            target_request_id,
                            Err(format!(
                                "{SSH_CONNECTION_TIMEOUT_LABEL} ({SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS} s)"
                            )),
                        );
                    }
                    async_cx
                        .update(|_, cx| {
                            _ = view.update(cx, |_, cx| cx.notify());
                        })
                        .ok();
                    break;
                }
            }
        })
        .detach();
    }

    /// Publish a remote-open outcome exactly once for a single open operation.
    ///
    /// ### Arguments
    /// - `open_finished`: Per-operation completion flag shared by the worker and monitor task.
    /// - `pending_remote`: Queue of remote-open outcomes drained by the render loop.
    /// - `target_tab_id`: Existing tab id for a reload operation, or `None` for new-tab opens.
    /// - `target_request_id`: Monotonic request token used to reject stale completions.
    /// - `outcome`: Success or failure result to enqueue.
    ///
    /// ### Returns
    /// - `true`: The outcome was accepted and enqueued.
    /// - `false`: Another outcome already won the race and this one was ignored.
    pub(super) fn publish_remote_open_outcome(
        open_finished: &AtomicBool,
        pending_remote: &Mutex<Vec<PendingRemoteOpenOutcome>>,
        target_tab_id: Option<usize>,
        target_request_id: Option<u64>,
        outcome: Result<RemoteOpenResult, String>,
    ) -> bool {
        if !open_finished.swap(true, Ordering::AcqRel) {
            pending_remote.lock().push(PendingRemoteOpenOutcome {
                target_tab_id,
                target_request_id,
                result: outcome,
            });
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{RemoteFileResult, RemoteOpenResult};
    use crate::fulgur::{Fulgur, sync::ssh::url::RemoteSpec, ui::components_utils::UTF_8};
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicBool;

    fn make_remote_result() -> RemoteOpenResult {
        RemoteOpenResult::File(RemoteFileResult {
            spec: RemoteSpec {
                host: "example.com".to_string(),
                port: 22,
                user: Some("alice".to_string()),
                path: "/tmp/test.txt".to_string(),
                password_in_url: None,
            },
            content: "ok".to_string(),
            encoding: UTF_8.to_string(),
            file_size: 2,
        })
    }

    #[test]
    fn test_publish_remote_open_outcome_ignores_timeout_after_success() {
        let open_finished = AtomicBool::new(false);
        let pending_remote = Mutex::new(Vec::new());

        let published_success = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            Ok(make_remote_result()),
        );
        let published_timeout = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            Err("SSH connection timed out (60 s)".to_string()),
        );

        assert!(published_success, "first outcome should win");
        assert!(!published_timeout, "timeout must be ignored after success");
        let queue = pending_remote.lock();
        assert_eq!(queue.len(), 1, "only one outcome must be queued");
        assert!(
            queue[0].result.is_ok(),
            "queued outcome should be the success result"
        );
    }

    #[test]
    fn test_publish_remote_open_outcome_ignores_success_after_timeout() {
        let open_finished = AtomicBool::new(false);
        let pending_remote = Mutex::new(Vec::new());

        let published_timeout = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            Err("SSH connection timed out (60 s)".to_string()),
        );
        let published_success = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            Ok(make_remote_result()),
        );

        assert!(published_timeout, "first outcome should win");
        assert!(
            !published_success,
            "late success must be ignored after timeout"
        );
        let queue = pending_remote.lock();
        assert_eq!(queue.len(), 1, "only one outcome must be queued");
        assert!(
            queue[0].result.is_err(),
            "queued outcome should be the timeout result"
        );
    }
}
