use super::{
    remote_ssh_task::{SshTaskContext, spawn_ssh_task},
    remote_types::{RemoteSaveRequest, RemoteSaveTaskParams, SSH_SAVE_TIMEOUT_LABEL},
};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, sync::ssh, ui::notifications::progress::CancelCallback,
};
use gpui_kit::component::{WindowExt, notification::NotificationType};
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
    /// - `request`: The tab, remote location, content snapshot and optional continuation
    pub(crate) fn save_remote_file(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        request: RemoteSaveRequest,
    ) {
        let RemoteSaveRequest {
            tab_id,
            spec,
            contents,
            bytes,
            on_saved,
        } = request;
        let remote_save_queue = Arc::clone(&Fulgur::shared_state(cx).remote_save_queue);
        let saved_content = Arc::new(contents);
        let saved_bytes = Arc::new(bytes);

        self.with_resolved_remote_user(window, cx, spec, move |fulgur, spec, window, cx| {
            if let Some(tab_entity) = fulgur.tab_entity_of(tab_id, cx) {
                tab_entity.update(cx, |tab, _| {
                    if let Some(editor_tab) = tab.as_editor_mut()
                        && let TabLocation::Remote(remote_spec) = &mut editor_tab.location
                        && remote_spec.user != spec.user
                    {
                        remote_spec.user.clone_from(&spec.user);
                    }
                });
            }
            let request_id = fulgur.next_remote_request_id;
            fulgur.next_remote_request_id = fulgur.next_remote_request_id.wrapping_add(1);
            fulgur
                .latest_remote_save_request_by_tab
                .insert(tab_id, request_id);
            let remote_save_permit = remote_save_queue.enqueue(&spec);
            Self::spawn_ssh_save_task(
                window,
                cx,
                RemoteSaveTaskParams {
                    tab_id,
                    request_id,
                    spec,
                    saved_content: Arc::clone(&saved_content),
                    saved_bytes: Arc::clone(&saved_bytes),
                    remote_save_permit,
                    on_saved: on_saved.clone(),
                },
            );
        });
    }

    /// Spawn a blocking SSH/SFTP save worker with host-key UI monitoring.
    ///
    /// ### Arguments
    /// - `window`: The window context used to spawn the async monitor task
    /// - `cx`: The application context
    /// - `params`: All data required to run the remote save operation
    fn spawn_ssh_save_task(
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        params: RemoteSaveTaskParams,
    ) {
        let RemoteSaveTaskParams {
            tab_id,
            request_id,
            spec,
            saved_content,
            saved_bytes,
            remote_save_permit,
            on_saved,
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
                progress_prefix: "Saving to ",
                operation: "save",
                timeout_label: SSH_SAVE_TIMEOUT_LABEL,
                cancel_callback,
            },
            move |session, spec, cancelled| {
                remote_save_permit.wait();
                // A save queued behind a slow one may have been cancelled while it waited.
                if cancelled.load(Ordering::Acquire) {
                    return Err(ssh::error::SshError::Cancelled);
                }
                ssh::sftp::write_remote_file(session, &spec.path, content_for_thread.as_slice())
            },
            move |save_finished, outcome| {
                Self::publish_remote_save_outcome(save_finished, &pending_save_for_publish, outcome)
            },
            move |_save_finished| pending_save_result.lock().take(),
            move |fulgur, result, window, cx| {
                let saved = fulgur.handle_remote_save_result(
                    tab_id,
                    request_id,
                    saved_content.as_str(),
                    result,
                    window,
                    cx,
                );
                if saved && let Some(on_saved) = &on_saved {
                    on_saved(fulgur, window, cx);
                }
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
    /// - `window`: The window used to report a failure
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the save succeeded and the tab's baseline was updated
    pub(super) fn handle_remote_save_result(
        &mut self,
        tab_id: TabId,
        request_id: u64,
        saved_content: &str,
        result: Result<(), String>,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        if self.latest_remote_save_request_by_tab.get(&tab_id).copied() != Some(request_id) {
            return false;
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
                        editor_tab.record_file_metadata(
                            saved_content.len(),
                            Some(std::time::SystemTime::now()),
                        );
                        cx.notify();
                    })
                    .is_some();
                if updated {
                    self.inflight_remote_restore.remove(&tab_id);
                    cx.notify();
                    crate::fulgur::state::schedule_session_autosave(cx);
                }
                updated
            }
            Err(msg) => {
                window.push_notification(
                    (
                        NotificationType::Error,
                        gpui_kit::SharedString::from(format!("Failed to save: {msg}")),
                    ),
                    cx,
                );
                false
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
