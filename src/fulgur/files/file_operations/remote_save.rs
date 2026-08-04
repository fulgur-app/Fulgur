use super::{
    remote_ssh_task::{SshTaskContext, spawn_ssh_task},
    remote_types::{RemoteSaveTaskParams, SSH_SAVE_TIMEOUT_LABEL},
};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur,
    editor_tab::TabLocation,
    sync::ssh::{self, credentials::SshCredKey, url::RemoteSpec},
    ui::notifications::progress::CancelCallback,
};
use gpui_component::{WindowExt, notification::NotificationType};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

impl Fulgur {
    /// Save a remote tab by resolving credentials then spawning an SSH/SFTP worker.
    ///
    /// ### Arguments
    /// - `window`: The window used to spawn dialog and monitoring tasks
    /// - `cx`: The application context
    /// - `tab_id`: Stable editor-tab id used to apply completion updates
    /// - `spec`: Remote file specification for the tab
    /// - `contents`: Snapshot of editor text, used to reset the modified baseline
    /// - `bytes`: The encoded file contents to write to the remote host
    pub(super) fn save_remote_file(
        &mut self,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        tab_id: TabId,
        mut spec: RemoteSpec,
        contents: String,
        bytes: Vec<u8>,
    ) {
        let ssh_session_cache = Arc::clone(&Fulgur::shared_state(cx).ssh_session_cache);
        let ssh_session_pool = Arc::clone(&Fulgur::shared_state(cx).ssh_session_pool);
        if let (Some(user), Some(password)) = (spec.user.clone(), spec.password_in_url.take()) {
            let key = SshCredKey::new(spec.host.clone(), spec.port, user);
            ssh_session_cache.lock().insert(key, password);
        }

        let saved_content = Arc::new(contents);
        let saved_bytes = Arc::new(bytes);
        if let Some(user) = spec.user.clone() {
            let cache_key = SshCredKey::new(spec.host.clone(), spec.port, user.clone());
            if let Some(cached_password) = ssh_session_cache.lock().get(&cache_key).cloned() {
                spec.password_in_url = None;
                let request_id = self.next_remote_request_id;
                self.next_remote_request_id = self.next_remote_request_id.wrapping_add(1);
                self.latest_remote_save_request_by_tab
                    .insert(tab_id, request_id);
                Self::spawn_ssh_save_task(
                    window,
                    cx,
                    RemoteSaveTaskParams {
                        tab_id,
                        request_id,
                        spec,
                        saved_content: Arc::clone(&saved_content),
                        saved_bytes: Arc::clone(&saved_bytes),
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
            &host,
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
                        if let Some(tab_entity) = fulgur.tab_entity_of(tab_id, cx) {
                            tab_entity.update(cx, |tab, _| {
                                if let Some(editor_tab) = tab.as_editor_mut()
                                    && let TabLocation::Remote(remote_spec) =
                                        &mut editor_tab.location
                                {
                                    remote_spec.user = Some(resolved_user.clone());
                                }
                            });
                        }
                        let request_id = fulgur.next_remote_request_id;
                        fulgur.next_remote_request_id =
                            fulgur.next_remote_request_id.wrapping_add(1);
                        fulgur
                            .latest_remote_save_request_by_tab
                            .insert(tab_id, request_id);
                        Self::spawn_ssh_save_task(
                            window,
                            cx,
                            RemoteSaveTaskParams {
                                tab_id,
                                request_id,
                                spec: spec_with_user,
                                saved_content: Arc::clone(&saved_content),
                                saved_bytes: Arc::clone(&saved_bytes),
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
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
        params: RemoteSaveTaskParams,
    ) {
        let RemoteSaveTaskParams {
            tab_id,
            request_id,
            spec,
            saved_content,
            saved_bytes,
            password,
            credential_key,
            ssh_session_cache,
            ssh_session_pool,
        } = params;
        let pending_save_result: Arc<Mutex<Option<Result<(), String>>>> =
            Arc::new(Mutex::new(None));
        let pending_save_for_publish = Arc::clone(&pending_save_result);
        let content_for_thread = Arc::clone(&saved_bytes);

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
        spawn_ssh_task(
            window,
            cx,
            SshTaskContext {
                spec,
                password,
                credential_key,
                ssh_session_cache,
                ssh_session_pool,
                progress_prefix: "Saving to ",
                timeout_label: SSH_SAVE_TIMEOUT_LABEL,
                cancel_callback,
            },
            move |session, spec| {
                ssh::sftp::write_remote_file(session, &spec.path, content_for_thread.as_slice())
            },
            move |save_finished, outcome| {
                Self::publish_remote_save_outcome(save_finished, &pending_save_for_publish, outcome)
            },
            move |_save_finished| pending_save_result.lock().take(),
            move |fulgur, result, window, cx| {
                fulgur.handle_remote_save_result(
                    tab_id,
                    request_id,
                    saved_content.as_str(),
                    result,
                    window,
                    cx,
                );
            },
        );
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
        if save_finished.swap(true, Ordering::AcqRel) {
            false
        } else {
            *pending_save.lock() = Some(outcome);
            true
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
        tab_id: TabId,
        request_id: u64,
        saved_content: &str,
        result: Result<(), String>,
        window: &mut gpui::Window,
        cx: &mut gpui::Context<Self>,
    ) {
        if self.latest_remote_save_request_by_tab.get(&tab_id).copied() != Some(request_id) {
            return;
        }
        self.latest_remote_save_request_by_tab.remove(&tab_id);

        match result {
            Ok(()) => {
                let updated = self
                    .update_editor_tab(tab_id, cx, |editor_tab, cx| {
                        // Keep async save semantics correct: if content changed after dispatch,
                        // this remains dirty because baseline is set to the persisted snapshot.
                        editor_tab.set_original_content_from_str(saved_content);
                        editor_tab.modified = false;
                        editor_tab.modified = editor_tab.content_differs_from_original(cx);
                        editor_tab.update_file_tooltip_cache(saved_content.len());
                        cx.notify();
                    })
                    .is_some();
                if updated {
                    self.pending_remote_restore.remove(&tab_id);
                    self.inflight_remote_restore.remove(&tab_id);
                    cx.notify();
                }
            }
            Err(msg) => {
                window.push_notification(
                    (
                        NotificationType::Error,
                        gpui::SharedString::from(format!("Failed to save: {msg}")),
                    ),
                    cx,
                );
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
