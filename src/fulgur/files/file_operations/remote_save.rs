use super::{
    DiskChangePolicy,
    remote_ssh_task::{SshTaskContext, spawn_ssh_task},
    remote_types::{RemoteSaveRequest, RemoteSaveTaskParams, SSH_SAVE_TIMEOUT_LABEL},
};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur,
    editor_tab::TabLocation,
    sync::ssh::{
        self,
        sftp::{RemoteBaseline, RemoteConflict, RemoteWriteGuard, RemoteWriteOutcome},
    },
    ui::{dialogs::save_conflict::ConflictedFile, notifications::progress::CancelCallback},
};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

/// Decide what a save attempt requires of the remote file it is about to replace.
///
/// ### Arguments
/// - `policy`: What the save does if the file changed on the server
/// - `baseline`: Version of the remote file the tab's buffer is based on
/// - `already_verified`: Whether an earlier attempt of the same save found the file
///   unchanged. That attempt may have been interrupted while writing, so a difference
///   from the baseline is then this save's own doing.
///
/// ### Returns
/// - `RemoteWriteGuard`: The guard to write under
fn remote_write_guard(
    policy: DiskChangePolicy,
    baseline: &RemoteBaseline,
    already_verified: bool,
) -> RemoteWriteGuard {
    match policy {
        DiskChangePolicy::Prompt if !already_verified => {
            RemoteWriteGuard::Unchanged(baseline.get())
        }
        DiskChangePolicy::Prompt | DiskChangePolicy::Overwrite => RemoteWriteGuard::Overwrite,
    }
}

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
            disk_change_policy,
            on_saved,
        } = request;
        let baseline = self
            .tab_entity_of(tab_id, cx)
            .and_then(|tab| Some(tab.read(cx).as_editor()?.remote_baseline.clone()))
            .unwrap_or_default();
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
                    disk_change_policy,
                    baseline: baseline.clone(),
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
            disk_change_policy,
            baseline,
            remote_save_permit,
            on_saved,
        } = params;
        let pending_save_result: Arc<Mutex<Option<Result<RemoteWriteOutcome, String>>>> =
            Arc::new(Mutex::new(None));
        let destination_verified = AtomicBool::new(false);
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
                let guard = remote_write_guard(
                    disk_change_policy,
                    &baseline,
                    destination_verified.load(Ordering::Acquire),
                );
                let outcome = ssh::sftp::write_remote_file(
                    session,
                    &spec.path,
                    content_for_thread.as_slice(),
                    guard,
                    || destination_verified.store(true, Ordering::Release),
                )?;
                if let RemoteWriteOutcome::Written(version) = outcome {
                    baseline.set(Some(version));
                }
                Ok(outcome)
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
    pub(super) fn publish_remote_save_outcome<Outcome>(
        save_finished: &AtomicBool,
        pending_save: &Mutex<Option<Result<Outcome, String>>>,
        outcome: Result<Outcome, String>,
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
    /// - `saved_content`: Snapshot that was sent to the remote host
    /// - `result`: Save outcome from the worker task
    /// - `window`: The window used to report a failure or a conflict
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the file was written and the tab's baseline was updated
    pub(super) fn handle_remote_save_result(
        &mut self,
        tab_id: TabId,
        request_id: u64,
        saved_content: &str,
        result: Result<RemoteWriteOutcome, String>,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        if self.latest_remote_save_request_by_tab.get(&tab_id).copied() != Some(request_id) {
            return false;
        }
        self.latest_remote_save_request_by_tab.remove(&tab_id);

        match result {
            Ok(RemoteWriteOutcome::Written(version)) => {
                let updated = self
                    .update_editor_tab(tab_id, cx, |editor_tab, cx| {
                        // Keep async save semantics correct: if content changed after dispatch,
                        // this remains dirty because baseline is set to the persisted snapshot.
                        editor_tab.set_original_content_from_str(saved_content);
                        editor_tab.modified = false;
                        editor_tab.modified = editor_tab.content_differs_from_original(cx);
                        editor_tab.file_size_bytes = version.size;
                        editor_tab.record_disk_version(version.modified());
                        cx.notify();
                    })
                    .is_some();
                if updated {
                    self.inflight_content_loads.remove(&tab_id);
                    cx.notify();
                    crate::fulgur::state::schedule_session_autosave(cx);
                }
                updated
            }
            Ok(RemoteWriteOutcome::Refused(conflict)) => {
                self.report_remote_save_conflict(tab_id, conflict, window, cx);
                false
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

    /// Ask the user how to resolve a remote save refused because of the version the
    /// server holds.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab whose save was refused
    /// - `conflict`: Why the save was refused
    /// - `window`: The window owning the tab
    /// - `cx`: The application context
    fn report_remote_save_conflict(
        &mut self,
        tab_id: TabId,
        conflict: RemoteConflict,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let location = self
            .tab_entity_of(tab_id, cx)
            .and_then(|tab| Some(tab.read(cx).as_editor()?.location.clone()));
        let Some(TabLocation::Remote(spec)) = location else {
            return;
        };
        log::warn!(
            "Not saving {} on {}: {}",
            spec.path,
            spec.host,
            match conflict {
                RemoteConflict::Changed => "it changed since it was last loaded or saved",
                RemoteConflict::UnknownBaseline => "the version the edits are based on is unknown",
            }
        );
        self.show_save_conflict_dialog(
            ConflictedFile::Remote { spec, conflict },
            tab_id,
            window,
            cx,
        );
    }
}

#[cfg(test)]
mod tests {
    use super::{Fulgur, remote_write_guard};
    use crate::fulgur::files::file_operations::DiskChangePolicy;
    use crate::fulgur::sync::ssh::sftp::{RemoteBaseline, RemoteFileVersion, RemoteWriteGuard};
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

    #[test]
    fn test_prompting_save_requires_the_version_current_at_write_time() {
        let baseline = RemoteBaseline::new(None);
        let worker_baseline = baseline.clone();
        assert_eq!(
            remote_write_guard(DiskChangePolicy::Prompt, &worker_baseline, false),
            RemoteWriteGuard::Unchanged(None)
        );

        let written = RemoteFileVersion {
            size: Some(4),
            mtime: Some(1_000),
        };
        baseline.set(Some(written));

        assert_eq!(
            remote_write_guard(DiskChangePolicy::Prompt, &worker_baseline, false),
            RemoteWriteGuard::Unchanged(Some(written)),
            "a save queued behind another one must expect the version that one wrote"
        );
    }

    #[test]
    fn test_retry_of_a_verified_save_does_not_conflict_with_its_own_write() {
        assert_eq!(
            remote_write_guard(DiskChangePolicy::Prompt, &RemoteBaseline::default(), true),
            RemoteWriteGuard::Overwrite
        );
    }

    #[test]
    fn test_overwriting_save_ignores_the_baseline() {
        assert_eq!(
            remote_write_guard(
                DiskChangePolicy::Overwrite,
                &RemoteBaseline::default(),
                false
            ),
            RemoteWriteGuard::Overwrite
        );
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod conflict_tests {
    use crate::fulgur::{
        Fulgur,
        editor_tab::{TabLocation, replace_editor_text},
        files::file_operations::test_helpers::setup_fulgur_with_root,
        sync::ssh::{
            sftp::{RemoteConflict, RemoteFileVersion, RemoteWriteOutcome},
            url::RemoteSpec,
        },
        ui::tabs::tab::TabId,
    };
    use crate::test_support::{click_dialog_button, has_active_dialog};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};

    const REQUEST_ID: u64 = 7;
    const LOCAL_TEXT: &str = "local edits";

    /// Turn the active tab into a dirty remote tab whose save is in flight. Nothing
    /// listens on its port, so a connection it starts fails at once.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    ///
    /// ### Returns
    /// - `TabId`: The id of the prepared tab
    fn saving_remote_tab(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) -> TabId {
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.active_tab_entity(cx).expect("expected an active tab");
                let tab_id = tab.read(cx).id();
                tab.update(cx, |tab, cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.location = TabLocation::Remote(RemoteSpec {
                        host: "127.0.0.1".to_string(),
                        port: 1,
                        user: Some("alice".to_string()),
                        path: "/tmp/notes.txt".to_string(),
                        password_in_url: None,
                    });
                    editor.content.update(cx, |state, cx| {
                        replace_editor_text(state, LOCAL_TEXT, window, cx);
                    });
                });
                this.latest_remote_save_request_by_tab
                    .insert(tab_id, REQUEST_ID);
                tab_id
            })
        })
    }

    /// Deliver the outcome of the in-flight save.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `tab_id`: The tab being saved
    /// - `outcome`: The outcome reported by the worker
    ///
    /// ### Returns
    /// - `bool`: Whether the tab was recorded as saved
    fn deliver(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        tab_id: TabId,
        outcome: RemoteWriteOutcome,
    ) -> bool {
        let saved = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.handle_remote_save_result(
                    tab_id,
                    REQUEST_ID,
                    LOCAL_TEXT,
                    Ok(outcome),
                    window,
                    cx,
                )
            })
        });
        visual_cx.run_until_parked();
        saved
    }

    /// Raise the conflict dialog for a dirty remote tab.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `conflict`: Why the save was refused
    ///
    /// ### Returns
    /// - `TabId`: The id of the tab whose save was refused
    fn raise_conflict(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        conflict: RemoteConflict,
    ) -> TabId {
        let tab_id = saving_remote_tab(fulgur, visual_cx);
        let saved = deliver(
            fulgur,
            visual_cx,
            tab_id,
            RemoteWriteOutcome::Refused(conflict),
        );
        assert!(!saved, "a refused save must not run its continuation");
        assert!(has_active_dialog(visual_cx));
        tab_id
    }

    /// Read what a conflict answer started, and the tab's buffer.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `tab_id`: The tab to inspect
    ///
    /// ### Returns
    /// - `(bool, bool, String, bool)`: Whether a save and a reload are in flight, the
    ///   buffer text, and whether the tab is modified
    fn state_of(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        tab_id: TabId,
    ) -> (bool, bool, String, bool) {
        visual_cx.update(|_, cx| {
            let this = fulgur.read(cx);
            let tab = this.tab_entity_of(tab_id, cx).expect("expected the tab");
            let editor = tab.read(cx).as_editor().expect("expected an editor tab");
            (
                this.latest_remote_save_request_by_tab.contains_key(&tab_id),
                this.latest_remote_open_request_by_tab.contains_key(&tab_id),
                editor.content.read(cx).text().to_string(),
                editor.modified,
            )
        })
    }

    #[gpui_kit::test]
    fn test_refused_remote_save_keeps_the_edits_dirty(cx: &mut TestAppContext) {
        for conflict in [RemoteConflict::Changed, RemoteConflict::UnknownBaseline] {
            let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
            let tab_id = raise_conflict(&fulgur, &mut visual_cx, conflict);

            assert_eq!(
                state_of(&fulgur, &mut visual_cx, tab_id),
                (false, false, LOCAL_TEXT.to_string(), true)
            );
        }
    }

    #[gpui_kit::test]
    fn test_remote_conflict_cancel_does_nothing(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let tab_id = raise_conflict(&fulgur, &mut visual_cx, RemoteConflict::Changed);

        click_dialog_button(&mut visual_cx, "save-conflict-cancel");

        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            state_of(&fulgur, &mut visual_cx, tab_id),
            (false, false, LOCAL_TEXT.to_string(), true)
        );
    }

    #[gpui_kit::test]
    fn test_remote_conflict_overwrite_starts_a_save(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let tab_id = raise_conflict(&fulgur, &mut visual_cx, RemoteConflict::Changed);

        click_dialog_button(&mut visual_cx, "save-conflict-overwrite");

        assert!(!has_active_dialog(&mut visual_cx));
        let (saving, reloading, _, _) = state_of(&fulgur, &mut visual_cx, tab_id);
        assert!(saving);
        assert!(!reloading);
    }

    #[gpui_kit::test]
    fn test_remote_conflict_reload_starts_a_read(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let tab_id = raise_conflict(&fulgur, &mut visual_cx, RemoteConflict::Changed);
        let first_request_id = visual_cx.update(|_, cx| fulgur.read(cx).next_remote_request_id);

        click_dialog_button(&mut visual_cx, "save-conflict-load-from-file");

        assert!(!has_active_dialog(&mut visual_cx));
        let (saving, _, _, _) = state_of(&fulgur, &mut visual_cx, tab_id);
        assert!(!saving);
        // The read fails at once and its outcome may already be drained; the request
        // counter holds whatever the timing.
        visual_cx.update(|_, cx| {
            assert_eq!(fulgur.read(cx).next_remote_request_id, first_request_id + 1);
        });
    }

    #[gpui_kit::test]
    fn test_remote_conflict_answer_ignores_a_retargeted_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let tab_id = raise_conflict(&fulgur, &mut visual_cx, RemoteConflict::Changed);
        visual_cx.update(|_, cx| {
            fulgur.update(cx, |this, cx| {
                this.update_editor_tab(tab_id, cx, |editor, _| {
                    editor.location = TabLocation::Untitled;
                });
            });
        });

        click_dialog_button(&mut visual_cx, "save-conflict-overwrite");

        let (saving, reloading, _, _) = state_of(&fulgur, &mut visual_cx, tab_id);
        assert!(!saving && !reloading);
    }

    #[gpui_kit::test]
    fn test_written_remote_save_records_the_server_version(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let tab_id = saving_remote_tab(&fulgur, &mut visual_cx);
        let version = RemoteFileVersion {
            size: Some(11),
            mtime: Some(1_000),
        };

        assert!(deliver(
            &fulgur,
            &mut visual_cx,
            tab_id,
            RemoteWriteOutcome::Written(version)
        ));

        assert!(!has_active_dialog(&mut visual_cx));
        visual_cx.update(|_, cx| {
            let tab = fulgur
                .read(cx)
                .tab_entity_of(tab_id, cx)
                .expect("expected the tab");
            let editor = tab.read(cx).as_editor().expect("expected an editor tab");
            assert!(!editor.modified);
            assert_eq!(editor.file_size_bytes, Some(11));
            assert_eq!(editor.file_last_modified, version.modified());
        });
    }
}
