use super::remote_types::{
    RemoteSaveTaskParams, SSH_HOST_KEY_APPROVAL_TIMEOUT, SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS,
    SSH_SAVE_TIMEOUT_LABEL, format_remote_endpoint_label, wait_for_host_key_decision,
};
use crate::fulgur::{
    Fulgur,
    editor_tab::TabLocation,
    sync::ssh::{
        self,
        credentials::SshCredKey,
        session::{HostKeyDecision, HostKeyRequest},
        url::RemoteSpec,
    },
    tab::Tab,
    ui::notifications::progress::{CancelCallback, start_progress},
};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

impl Fulgur {
    /// Save a remote tab by resolving credentials then spawning an SSH/SFTP worker.
    ///
    /// ### Arguments
    /// - `window`: The window used to spawn dialog and monitoring tasks
    /// - `cx`: The application context
    /// - `tab_id`: Stable editor-tab id used to apply completion updates
    /// - `spec`: Remote file specification for the tab
    /// - `contents`: Snapshot of editor contents to persist remotely
    pub(super) fn save_remote_file(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        tab_id: usize,
        mut spec: RemoteSpec,
        contents: String,
    ) {
        let ssh_session_cache = Arc::clone(&self.shared_state(cx).ssh_session_cache);
        let ssh_session_pool = Arc::clone(&self.shared_state(cx).ssh_session_pool);
        if let (Some(user), Some(password)) = (spec.user.clone(), spec.password_in_url.take()) {
            let key = SshCredKey::new(spec.host.clone(), spec.port, user);
            ssh_session_cache.lock().insert(key, password);
        }

        let saved_content = Arc::new(contents);
        if let Some(user) = spec.user.clone() {
            let cache_key = SshCredKey::new(spec.host.clone(), spec.port, user.clone());
            if let Some(cached_password) = ssh_session_cache.lock().get(&cache_key).cloned() {
                spec.password_in_url = None;
                let request_id = self.next_remote_request_id;
                self.next_remote_request_id = self.next_remote_request_id.wrapping_add(1);
                self.latest_remote_save_request_by_tab
                    .insert(tab_id, request_id);
                self.spawn_ssh_save_task(
                    window,
                    cx,
                    RemoteSaveTaskParams {
                        tab_id,
                        request_id,
                        spec,
                        saved_content: Arc::clone(&saved_content),
                        password: cached_password,
                        credential_key: cache_key,
                        ssh_session_cache: Arc::clone(&ssh_session_cache),
                        ssh_session_pool: Arc::clone(&ssh_session_pool),
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
            host,
            port,
            user,
            move |resolved_user, password, window, cx| {
                let mut spec_with_user = spec.clone();
                spec_with_user.user = Some(resolved_user.clone());
                spec_with_user.password_in_url = None;
                let cache_key = SshCredKey::new(
                    spec_with_user.host.clone(),
                    spec_with_user.port,
                    resolved_user.clone(),
                );
                if let Some(entity) = entity.upgrade() {
                    entity.update(cx, |fulgur, cx| {
                        cache_for_callback
                            .lock()
                            .insert(cache_key.clone(), password.clone());
                        if let Some(Tab::Editor(editor_tab)) =
                            fulgur.tabs.iter_mut().find(|tab| tab.id() == tab_id)
                            && let TabLocation::Remote(remote_spec) = &mut editor_tab.location
                        {
                            remote_spec.user = Some(resolved_user.clone());
                        }
                        let request_id = fulgur.next_remote_request_id;
                        fulgur.next_remote_request_id =
                            fulgur.next_remote_request_id.wrapping_add(1);
                        fulgur
                            .latest_remote_save_request_by_tab
                            .insert(tab_id, request_id);
                        fulgur.spawn_ssh_save_task(
                            window,
                            cx,
                            RemoteSaveTaskParams {
                                tab_id,
                                request_id,
                                spec: spec_with_user,
                                saved_content: Arc::clone(&saved_content),
                                password,
                                credential_key: cache_key,
                                ssh_session_cache: Arc::clone(&cache_for_callback),
                                ssh_session_pool: Arc::clone(&pool_for_callback),
                            },
                        );
                    });
                }
            },
        );
    }

    /// Spawn a blocking SSH/SFTP save worker with host-key UI monitoring.
    ///
    /// ### Arguments
    /// - `window`: The window context used to spawn the async monitor task
    /// - `cx`: The application context
    /// - `params`: All data required to run the remote save operation
    fn spawn_ssh_save_task(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        params: RemoteSaveTaskParams,
    ) {
        let RemoteSaveTaskParams {
            tab_id,
            request_id,
            spec,
            saved_content,
            password,
            credential_key,
            ssh_session_cache,
            ssh_session_pool,
        } = params;
        let pending_host_key: Arc<Mutex<Option<HostKeyRequest>>> = Arc::new(Mutex::new(None));
        let pending_host_key_for_thread = Arc::clone(&pending_host_key);
        let pending_host_key_for_task = Arc::clone(&pending_host_key);

        let pending_save_result: Arc<Mutex<Option<Result<(), String>>>> =
            Arc::new(Mutex::new(None));
        let pending_save_for_thread = Arc::clone(&pending_save_result);
        let pending_save_for_task = Arc::clone(&pending_save_result);

        let save_finished = Arc::new(AtomicBool::new(false));
        let save_finished_for_thread = Arc::clone(&save_finished);
        let save_finished_for_task = Arc::clone(&save_finished);
        let host_key_decision_timed_out = Arc::new(AtomicBool::new(false));
        let host_key_decision_timed_out_for_thread = Arc::clone(&host_key_decision_timed_out);

        let spec_for_thread = spec.clone();
        let user = spec.user.clone().unwrap_or_default();
        let cache_for_thread = Arc::clone(&ssh_session_cache);
        let credential_key_for_thread = credential_key.clone();
        let content_for_thread = Arc::clone(&saved_content);
        let pool_for_thread = Arc::clone(&ssh_session_pool);

        let progress_label =
            format_remote_endpoint_label("Saving to ", &spec.host, spec.port, &user);
        let entity_weak = cx.entity().downgrade();
        let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, cx| {
            if let Some(entity) = entity_weak.upgrade() {
                entity.update(cx, |fulgur, _cx| {
                    if fulgur
                        .latest_remote_save_request_by_tab
                        .get(&tab_id)
                        .copied()
                        == Some(request_id)
                    {
                        fulgur.latest_remote_save_request_by_tab.remove(&tab_id);
                    }
                });
            }
        }));
        let progress = start_progress(window, cx, progress_label.into(), cancel_callback);

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
                    wait_for_host_key_decision(rx, &host_key_decision_timed_out_for_callback)
                },
            );
            if let Err(ssh::error::SshError::AuthFailed) = &session_result {
                cache_for_thread.lock().remove(&credential_key_for_thread);
            }
            let mut outcome = session_result
                .and_then(|pooled_session| {
                    let result = ssh::sftp::write_remote_file(
                        pooled_session.session(),
                        &spec_for_thread.path,
                        content_for_thread.as_bytes(),
                    );
                    if result.is_err() {
                        pooled_session.invalidate();
                    }
                    result
                })
                .map_err(|e| e.user_message());
            if host_key_decision_timed_out_for_thread.load(Ordering::Acquire) {
                outcome = Err(format!(
                    "{SSH_SAVE_TIMEOUT_LABEL} ({SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS} s)"
                ));
            }
            Self::publish_remote_save_outcome(
                &save_finished_for_thread,
                &pending_save_for_thread,
                outcome,
            );
        });

        cx.spawn_in(window, async move |view, async_cx| {
            let _progress = progress;
            let deadline = std::time::Instant::now() + SSH_HOST_KEY_APPROVAL_TIMEOUT;
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;

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

                let save_result = pending_save_for_task.lock().take();
                if let Some(result) = save_result {
                    let saved_content = Arc::clone(&saved_content);
                    async_cx
                        .update(|_, cx| {
                            _ = view.update(cx, |fulgur, cx| {
                                fulgur.handle_remote_save_result(
                                    tab_id,
                                    request_id,
                                    saved_content.as_str(),
                                    result,
                                    cx,
                                );
                            });
                        })
                        .ok();
                    break;
                }

                if std::time::Instant::now() > deadline {
                    if let Some(request) = pending_host_key_for_task.lock().take() {
                        let _ = request.decision_tx.send(HostKeyDecision::Reject);
                    }
                    Self::publish_remote_save_outcome(
                        &save_finished_for_task,
                        &pending_save_for_task,
                        Err(format!(
                            "{SSH_SAVE_TIMEOUT_LABEL} ({SSH_HOST_KEY_APPROVAL_TIMEOUT_SECS} s)"
                        )),
                    );
                }
            }
        })
        .detach();
    }

    /// Publish a remote-save outcome exactly once for a single save operation.
    ///
    /// ### Arguments
    /// - `save_finished`: Per-operation completion flag shared by worker and monitor
    /// - `pending_save`: Shared slot consumed by the monitor task
    /// - `outcome`: Save result to publish
    ///
    /// ### Returns
    /// - `true`: The outcome was accepted and stored
    /// - `false`: Another outcome already won the race and this one was ignored
    pub(super) fn publish_remote_save_outcome(
        save_finished: &AtomicBool,
        pending_save: &Mutex<Option<Result<(), String>>>,
        outcome: Result<(), String>,
    ) -> bool {
        if !save_finished.swap(true, Ordering::AcqRel) {
            *pending_save.lock() = Some(outcome);
            true
        } else {
            false
        }
    }

    /// Apply a completed remote-save result on the UI thread.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable editor-tab id associated with the save request
    /// - `request_id`: Monotonic save request token used to ignore stale completions
    /// - `saved_content`: Snapshot that was successfully sent to the remote host
    /// - `result`: Save outcome from the worker task
    /// - `cx`: The application context
    pub(super) fn handle_remote_save_result(
        &mut self,
        tab_id: usize,
        request_id: u64,
        saved_content: &str,
        result: Result<(), String>,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.latest_remote_save_request_by_tab.get(&tab_id).copied() != Some(request_id) {
            return;
        }
        self.latest_remote_save_request_by_tab.remove(&tab_id);

        match result {
            Ok(()) => {
                if let Some(editor_tab) = self.tabs.iter_mut().find_map(|tab| {
                    if let Tab::Editor(editor_tab) = tab {
                        (editor_tab.id == tab_id).then_some(editor_tab)
                    } else {
                        None
                    }
                }) {
                    // Keep async save semantics correct: if content changed after dispatch,
                    // this remains dirty because baseline is set to the persisted snapshot.
                    editor_tab.set_original_content_from_str(saved_content);
                    editor_tab.modified = editor_tab.content_differs_from_original(cx);
                    editor_tab.update_file_tooltip_cache(saved_content.len());
                    self.pending_remote_restore.remove(&tab_id);
                    self.inflight_remote_restore.remove(&tab_id);
                    cx.notify();
                }
            }
            Err(msg) => {
                self.pending_notification = Some((
                    NotificationType::Error,
                    format!("Failed to save: {msg}").into(),
                ));
                cx.notify();
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::Fulgur;
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicBool;

    #[test]
    fn test_publish_remote_save_outcome_ignores_timeout_after_success() {
        let save_finished = AtomicBool::new(false);
        let pending_save = Mutex::new(None);

        let published_success =
            Fulgur::publish_remote_save_outcome(&save_finished, &pending_save, Ok(()));
        let published_timeout = Fulgur::publish_remote_save_outcome(
            &save_finished,
            &pending_save,
            Err("SSH save timed out (60 s)".to_string()),
        );

        assert!(published_success, "first save outcome should win");
        assert!(!published_timeout, "timeout must be ignored after success");
        let result = pending_save.lock();
        assert!(result.is_some(), "one save outcome should be queued");
        assert!(result.as_ref().is_some_and(Result::is_ok));
    }

    #[test]
    fn test_publish_remote_save_outcome_ignores_success_after_timeout() {
        let save_finished = AtomicBool::new(false);
        let pending_save = Mutex::new(None);

        let published_timeout = Fulgur::publish_remote_save_outcome(
            &save_finished,
            &pending_save,
            Err("SSH save timed out (60 s)".to_string()),
        );
        let published_success =
            Fulgur::publish_remote_save_outcome(&save_finished, &pending_save, Ok(()));

        assert!(published_timeout, "first save outcome should win");
        assert!(
            !published_success,
            "late success must be ignored after timeout"
        );
        let result = pending_save.lock();
        assert!(result.is_some(), "one save outcome should be queued");
        assert!(result.as_ref().is_some_and(Result::is_err));
    }
}
