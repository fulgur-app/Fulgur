use super::{
    RemoteFileResult, RemoteOpenResult, binary_file_notice, remote_types::RemoteReloadGuard,
};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{Fulgur, editor_tab, tab::Tab};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{App, Context, Window};
use std::path::PathBuf;

impl Fulgur {
    /// Drain pending remote file results and open loaded content in new tabs.
    ///
    /// Called every render pass. When SSH background threads deliver results
    /// (success or error), this method consumes them and either opens new tabs with
    /// loaded content or shows error notifications.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn process_pending_remote_files(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let outcomes = std::mem::take(&mut *self.pending_remote_open.lock());
        if outcomes.is_empty() {
            return;
        }
        for outcome in outcomes {
            let target_tab_id = outcome.target_tab_id;
            let target_reload_guard = outcome.target_reload_guard;
            let history_url = outcome.history_url;
            if let Some(tab_id) = target_tab_id {
                if let Some(request_id) = outcome.target_request_id
                    && self.latest_remote_open_request_by_tab.get(&tab_id).copied()
                        != Some(request_id)
                {
                    // A newer request for this tab is already in flight; ignore stale completion.
                    continue;
                }
                if let Some(request_id) = outcome.target_request_id
                    && self.latest_remote_open_request_by_tab.get(&tab_id).copied()
                        == Some(request_id)
                {
                    self.latest_remote_open_request_by_tab.remove(&tab_id);
                }
                self.inflight_remote_restore.remove(&tab_id);
            }

            match outcome.result {
                Ok(RemoteOpenResult::File(remote_file)) => {
                    if let Some(tab_id) = target_tab_id {
                        if let Some(guard) = target_reload_guard {
                            self.apply_remote_reload_to_existing_tab(
                                tab_id,
                                &guard,
                                remote_file,
                                window,
                                cx,
                            );
                        }
                    } else {
                        self.last_failed_remote_open_url = None;
                        Self::record_remote_history_url_if_opened(
                            history_url,
                            &remote_file.spec,
                            cx,
                        );
                        let recent_remote_url =
                            crate::fulgur::sync::ssh::url::format_remote_url(&remote_file.spec);
                        log::debug!(
                            "Remote file loaded: {}:{}",
                            remote_file.spec.host,
                            remote_file.spec.path
                        );
                        let new_tab_id = self.allocate_tab_id();
                        let mut editor_tab = editor_tab::EditorTab::from_remote_loaded(
                            new_tab_id,
                            remote_file,
                            window,
                            cx,
                            &self.settings.editor_settings,
                        );
                        if Self::shared_state(cx).read_only_session {
                            editor_tab.make_read_only(cx);
                        }
                        self.place_editor_tab_reusing_scratch(Tab::Editor(editor_tab), window, cx);
                        self.focus_active_tab(window, cx);
                        self.record_recent_file(PathBuf::from(recent_remote_url), cx);
                        self.save_state_async(cx, window);
                        cx.notify();
                    }
                }
                Ok(RemoteOpenResult::Browse(browse)) => {
                    if target_tab_id.is_some() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                gpui_kit::SharedString::from(
                                    "Restored remote tab path is no longer a file",
                                ),
                            ),
                            cx,
                        );
                    } else {
                        Self::record_remote_history_url_if_opened(
                            history_url,
                            &browse.directory_spec,
                            cx,
                        );
                        self.show_remote_path_browser_dialog(window, cx, &browse);
                    }
                }
                Ok(RemoteOpenResult::Binary(spec)) => {
                    log::warn!(
                        "Refusing to open binary remote file: {}:{}",
                        spec.host,
                        spec.path
                    );
                    let file_name = spec.path.rsplit('/').next().unwrap_or(&spec.path);
                    window.push_notification(
                        (
                            NotificationType::Warning,
                            gpui_kit::SharedString::from(binary_file_notice(file_name)),
                        ),
                        cx,
                    );
                }
                Err(msg) => {
                    window.push_notification(
                        (NotificationType::Error, gpui_kit::SharedString::from(msg)),
                        cx,
                    );
                }
            }
        }
    }

    /// Record a dialog URL in the remote history once its exact location was opened.
    ///
    /// ### Arguments
    /// - `history_url`: URL submitted in the Open Remote File dialog, `None` for other opens
    /// - `opened`: Location actually opened, as a file or in the remote browser
    /// - `cx`: The application context
    fn record_remote_history_url_if_opened(
        history_url: Option<String>,
        opened: &crate::fulgur::sync::ssh::url::RemoteSpec,
        cx: &mut App,
    ) {
        let Some(history_url) = history_url else {
            return;
        };
        if crate::fulgur::sync::ssh::url::format_remote_history_url(opened) == history_url {
            Self::record_remote_history_url(history_url, cx);
        }
    }

    /// Apply fresh remote contents to an already-restored tab after lazy reconnect.
    ///
    /// A tab that changed while the remote read was pending is left untouched, and
    /// stays awaiting its remote content.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable editor tab id to update
    /// - `guard`: Content revision and source captured before the SSH read
    /// - `remote_file`: Loaded remote payload from SSH worker
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_remote_reload_to_existing_tab(
        &mut self,
        tab_id: TabId,
        guard: &RemoteReloadGuard,
        remote_file: RemoteFileResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab_entity) = self.tab_entity_of(tab_id, cx) else {
            return;
        };
        let can_apply = tab_entity.read(cx).as_editor().is_some_and(|editor| {
            editor.content_revision(cx) == guard.content_revision
                && matches!(
                    &editor.location,
                    crate::fulgur::editor_tab::TabLocation::Remote(spec)
                        if crate::fulgur::sync::ssh::url::format_remote_url(spec) == guard.source_url
                )
        });
        if !can_apply {
            log::warn!(
                "Discarding stale remote reload for tab {tab_id} because its source or content changed"
            );
            tab_entity.update(cx, |tab, cx| {
                if let Some(editor) = tab.as_editor_mut() {
                    editor.check_modified(cx);
                    cx.notify();
                }
            });
            window.push_notification(
                (
                    NotificationType::Warning,
                    gpui_kit::SharedString::from(
                        "Remote reload skipped because the tab changed while loading; local edits were kept",
                    ),
                ),
                cx,
            );
            return;
        }
        tab_entity.update(cx, |tab, cx| {
            let Some(editor_tab) = tab.as_editor_mut() else {
                return;
            };
            editor_tab.content.update(cx, |input_state, cx| {
                editor_tab::replace_editor_text(input_state, &remote_file.content, window, cx);
            });
            editor_tab.location =
                crate::fulgur::editor_tab::TabLocation::Remote(remote_file.spec.clone());
            editor_tab.encoding = remote_file.encoding;
            editor_tab.lossy_decode = remote_file.lossy;
            editor_tab.set_original_content_from_str(&remote_file.content);
            editor_tab.modified = false;
            editor_tab.finish_awaiting_remote_content(cx);
            editor_tab
                .record_file_metadata(remote_file.file_size, Some(std::time::SystemTime::now()));
            let filename = remote_file
                .spec
                .path
                .rsplit('/')
                .next()
                .unwrap_or(&remote_file.spec.path)
                .to_string();
            editor_tab.title = filename.into();
            let language = crate::fulgur::languages::supported_languages::language_from_content(
                editor_tab.title.as_ref(),
                &remote_file.content,
            );
            tab.force_language(cx, language);
            cx.notify();
        });
        cx.notify();
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{RemoteFileResult, RemoteOpenResult};
    use crate::fulgur::{
        Fulgur,
        editor_tab::TabLocation,
        files::file_operations::{
            PendingRemoteOpenOutcome, remote_types::RemoteReloadGuard,
            test_helpers::setup_fulgur_with_root,
        },
        sync::ssh::url::RemoteSpec,
        tab::Tab,
        ui::{components_utils::UTF_8, tabs::tab::TabId},
    };
    use gpui_kit::{Context, Entity, TestAppContext};

    /// Build the remote location shared by the restored tab and its delayed result.
    ///
    /// ### Returns
    /// - `RemoteSpec`: A deterministic remote file location for the test
    fn remote_spec() -> RemoteSpec {
        RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/tmp/reconnect.txt".to_string(),
            password_in_url: None,
        }
    }

    /// Run the dialog-URL recording rule against an opened location.
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `history_url`: URL submitted in the dialog, `None` for other opens
    /// - `opened`: Location actually opened
    ///
    /// ### Returns
    /// - `Vec<String>`: The remote history afterwards, most recent first
    fn history_after_opening(
        cx: &mut TestAppContext,
        history_url: Option<String>,
        opened: &RemoteSpec,
    ) -> Vec<String> {
        let (_fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        visual_cx.update(|_window, cx| {
            Fulgur::record_remote_history_url_if_opened(history_url, opened, cx);
            Fulgur::shared_state(cx).remote_history.urls().to_vec()
        })
    }

    #[gpui_kit::test]
    fn test_a_dialog_url_is_recorded_once_its_location_opens(cx: &mut TestAppContext) {
        let history = history_after_opening(
            cx,
            Some("ssh://alice@example.com/tmp/reconnect.txt".to_string()),
            &remote_spec(),
        );
        assert_eq!(history, ["ssh://alice@example.com/tmp/reconnect.txt"]);
    }

    #[gpui_kit::test]
    fn test_a_missing_path_shown_as_its_parent_directory_is_not_recorded(cx: &mut TestAppContext) {
        let mut parent_directory = remote_spec();
        parent_directory.path = "/tmp".to_string();
        let history = history_after_opening(
            cx,
            Some("ssh://alice@example.com/tmp/reconnect.txt".to_string()),
            &parent_directory,
        );
        assert_eq!(
            history,
            Vec::<String>::new(),
            "a mistyped path must not be offered again"
        );
    }

    #[gpui_kit::test]
    fn test_opens_from_outside_the_dialog_are_not_recorded(cx: &mut TestAppContext) {
        let history = history_after_opening(cx, None, &remote_spec());
        assert_eq!(history, Vec::<String>::new());
    }

    #[gpui_kit::test]
    fn test_remote_reconnect_keeps_edits_made_while_loading(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab_entity = this.tabs.first().expect("expected an editor tab").clone();
                let (tab_id, expected_revision) = tab_entity.update(cx, |tab, cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.location = TabLocation::Remote(remote_spec());
                    editor.set_original_content_from_str("");
                    editor.modified = false;
                    editor.mark_awaiting_remote_content(cx);
                    (editor.id, editor.content_revision(cx))
                });

                tab_entity.update(cx, |tab, cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.content.update(cx, |state, cx| {
                        state.set_value("local edit", window, cx);
                    });
                });
                this.pending_remote_open
                    .lock()
                    .push(PendingRemoteOpenOutcome {
                        target_tab_id: Some(tab_id),
                        target_request_id: None,
                        target_reload_guard: Some(RemoteReloadGuard {
                            content_revision: expected_revision,
                            source_url: crate::fulgur::sync::ssh::url::format_remote_url(
                                &remote_spec(),
                            ),
                        }),
                        history_url: None,
                        result: Ok(RemoteOpenResult::File(RemoteFileResult {
                            spec: remote_spec(),
                            content: "remote content".to_string(),
                            encoding: UTF_8.to_string(),
                            lossy: false,
                            file_size: 14,
                        })),
                    });
                this.process_pending_remote_files(window, cx);

                let editor = tab_entity
                    .read(cx)
                    .as_editor()
                    .expect("expected an editor tab");
                assert_eq!(editor.content.read(cx).text().to_string(), "local edit");
                assert!(editor.modified, "the intervening edit must remain dirty");
                assert!(
                    editor.awaiting_remote_content,
                    "a rejected stale reload must not mark the restored tab as refreshed"
                );
            });
        });
    }

    /// Turn the first tab into a clean restored remote tab awaiting its lazy reload.
    ///
    /// ### Arguments
    /// - `this`: The application under test
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `(Entity<Tab>, TabId, RemoteReloadGuard)`: The tab, its id and a guard matching its state
    fn restored_remote_tab(
        this: &mut Fulgur,
        cx: &mut Context<Fulgur>,
    ) -> (Entity<Tab>, TabId, RemoteReloadGuard) {
        let tab_entity = this.tabs.first().expect("expected an editor tab").clone();
        let (tab_id, content_revision) = tab_entity.update(cx, |tab, cx| {
            let editor = tab.as_editor_mut().expect("expected an editor tab");
            editor.location = TabLocation::Remote(remote_spec());
            editor.set_original_content_from_str("");
            editor.modified = false;
            editor.mark_awaiting_remote_content(cx);
            (editor.id, editor.content_revision(cx))
        });
        let guard = RemoteReloadGuard {
            content_revision,
            source_url: crate::fulgur::sync::ssh::url::format_remote_url(&remote_spec()),
        };
        (tab_entity, tab_id, guard)
    }

    #[gpui_kit::test]
    fn test_remote_reload_carries_the_lossy_decode_flag(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, tab_id, guard) = restored_remote_tab(this, cx);
                this.pending_remote_open
                    .lock()
                    .push(PendingRemoteOpenOutcome {
                        target_tab_id: Some(tab_id),
                        target_request_id: None,
                        target_reload_guard: Some(guard),
                        history_url: None,
                        result: Ok(RemoteOpenResult::File(RemoteFileResult {
                            spec: remote_spec(),
                            content: "caf\u{fffd}".to_string(),
                            encoding: UTF_8.to_string(),
                            lossy: true,
                            file_size: 4,
                        })),
                    });
                this.process_pending_remote_files(window, cx);

                let editor = tab_entity
                    .read(cx)
                    .as_editor()
                    .expect("expected an editor tab");
                assert!(
                    editor.lossy_decode,
                    "a lossy remote reload must keep the lossy-save confirmation armed"
                );
                assert!(!editor.awaiting_remote_content);
            });
        });
    }

    #[gpui_kit::test]
    fn test_binary_remote_reload_leaves_the_tab_untouched(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, tab_id, guard) = restored_remote_tab(this, cx);
                this.pending_remote_open
                    .lock()
                    .push(PendingRemoteOpenOutcome {
                        target_tab_id: Some(tab_id),
                        target_request_id: None,
                        target_reload_guard: Some(guard),
                        history_url: None,
                        result: Ok(RemoteOpenResult::Binary(remote_spec())),
                    });
                this.process_pending_remote_files(window, cx);

                let editor = tab_entity
                    .read(cx)
                    .as_editor()
                    .expect("expected an editor tab");
                assert_eq!(editor.content.read(cx).text().to_string(), "");
                assert!(!editor.modified);
                assert!(
                    editor.awaiting_remote_content,
                    "a refused binary reload must leave the tab pending a retry"
                );
            });
        });
    }

    /// Point a tab at a port nothing listens on, so that a read it starts fails at once
    /// instead of reaching the network.
    ///
    /// ### Arguments
    /// - `tab_entity`: The tab to update
    /// - `cx`: The application context
    fn point_at_unreachable_host(tab_entity: &Entity<Tab>, cx: &mut Context<Fulgur>) {
        tab_entity.update(cx, |tab, _cx| {
            let editor = tab.as_editor_mut().expect("expected an editor tab");
            editor.location = TabLocation::Remote(RemoteSpec {
                host: "127.0.0.1".to_string(),
                port: 1,
                ..remote_spec()
            });
        });
    }

    #[gpui_kit::test]
    fn test_save_refuses_a_placeholder_and_starts_loading_it(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, tab_id, _guard) = restored_remote_tab(this, cx);
                point_at_unreachable_host(&tab_entity, cx);
                this.active_tab_id = Some(tab_id);

                this.save_file(window, cx);

                assert!(
                    !this.latest_remote_save_request_by_tab.contains_key(&tab_id),
                    "an unloaded buffer must never be written over the remote file"
                );
                assert!(this.inflight_remote_restore.contains(&tab_id));
            });
        });
    }

    #[gpui_kit::test]
    fn test_save_as_refuses_a_placeholder_without_asking_for_a_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (_tab_entity, tab_id, _guard) = restored_remote_tab(this, cx);
                this.inflight_remote_restore.insert(tab_id);
                this.active_tab_id = Some(tab_id);

                this.save_file_as(window, cx);
            });
        });

        assert!(!visual_cx.did_prompt_for_new_path());
    }

    #[gpui_kit::test]
    fn test_save_writes_recovered_edits_of_a_restored_remote_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab_entity = this.tabs.first().expect("expected an editor tab").clone();
                let tab_id = tab_entity.read(cx).id();
                tab_entity.update(cx, |tab, cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.content.update(cx, |state, cx| {
                        state.set_value("recovered edits", window, cx);
                    });
                    editor.mark_saved_baseline_unknown();
                });
                point_at_unreachable_host(&tab_entity, cx);
                this.active_tab_id = Some(tab_id);

                this.save_file(window, cx);

                assert!(
                    this.latest_remote_save_request_by_tab.contains_key(&tab_id),
                    "recovered edits are real content and must stay saveable"
                );
                assert!(
                    !this.inflight_remote_restore.contains(&tab_id),
                    "a reload would replace the recovered edits"
                );
            });
        });
    }

    #[gpui_kit::test]
    fn test_placeholder_refuses_edits_until_its_content_loads(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, tab_id, guard) = restored_remote_tab(this, cx);
                let editable = |tab: &Entity<Tab>, cx: &Context<Fulgur>| {
                    tab.read(cx)
                        .as_editor()
                        .expect("expected an editor tab")
                        .content
                        .read(cx)
                        .is_editable()
                };
                assert!(!editable(&tab_entity, cx));

                this.pending_remote_open
                    .lock()
                    .push(PendingRemoteOpenOutcome {
                        target_tab_id: Some(tab_id),
                        target_request_id: None,
                        target_reload_guard: Some(guard),
                        history_url: None,
                        result: Ok(RemoteOpenResult::File(RemoteFileResult {
                            spec: remote_spec(),
                            content: "remote content".to_string(),
                            encoding: UTF_8.to_string(),
                            lossy: false,
                            file_size: 14,
                        })),
                    });
                this.process_pending_remote_files(window, cx);

                assert!(editable(&tab_entity, cx));
                let editor = tab_entity
                    .read(cx)
                    .as_editor()
                    .expect("expected an editor tab");
                assert_eq!(editor.content.read(cx).text().to_string(), "remote content");
                assert!(!editor.modified);
            });
        });
    }

    #[gpui_kit::test]
    fn test_loaded_placeholder_stays_locked_when_the_tab_is_read_only(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, _tab_id, _guard) = restored_remote_tab(this, cx);
                tab_entity.update(cx, |tab, cx| {
                    let editor = tab.as_editor_mut().expect("expected an editor tab");
                    editor.make_read_only(cx);
                    editor.finish_awaiting_remote_content(cx);
                    assert!(!editor.content.read(cx).is_editable());
                });
            });
        });
    }

    #[gpui_kit::test]
    fn test_closing_the_active_tab_loads_the_placeholder_beside_it(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        let (tab_id, first_request_id) = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, tab_id, _guard) = restored_remote_tab(this, cx);
                point_at_unreachable_host(&tab_entity, cx);
                this.new_tab(window, cx);
                let new_tab_id = this
                    .active_tab_id
                    .expect("expected the new tab to be active");
                let first_request_id = this.next_remote_request_id;
                this.close_tab(new_tab_id, window, cx);
                (tab_id, first_request_id)
            })
        });

        // The read starts in a deferred effect, so the window draws before the next
        // check, and a draw drains whatever outcome the worker thread already
        // published. `inflight_remote_restore` is cleared by that drain; the request
        // counter is not, so it is the only witness that holds whatever the timing.
        visual_cx.update(|_window, cx| {
            let this = fulgur.read(cx);
            assert_eq!(this.active_tab_id, Some(tab_id));
            assert_eq!(
                this.next_remote_request_id,
                first_request_id + 1,
                "exactly one remote read must start, for the placeholder that became active"
            );
        });
    }

    #[gpui_kit::test]
    fn test_bulk_close_does_not_connect_for_tabs_it_passes_through(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        let (first_request_id, kept_tab_id) = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                restored_remote_tab(this, cx);
                this.new_tab(window, cx);
                let kept_tab_id = this
                    .active_tab_id
                    .expect("expected the new tab to be active");
                this.new_tab(window, cx);
                let moved_tab = this.tabs.pop().expect("expected the last tab");
                this.tabs.insert(0, moved_tab);
                // Tabs are now [active, placeholder, kept]: closing the active tab
                // briefly activates the placeholder, which is closed right after.
                let first_request_id = this.next_remote_request_id;
                this.close_other_tabs(kept_tab_id, window, cx);
                (first_request_id, kept_tab_id)
            })
        });

        visual_cx.update(|_window, cx| {
            let this = fulgur.read(cx);
            assert_eq!(this.active_tab_id, Some(kept_tab_id));
            assert_eq!(
                this.next_remote_request_id, first_request_id,
                "no remote read may start for a tab that is closing"
            );
        });
    }

    #[gpui_kit::test]
    fn test_transferred_placeholder_loads_in_the_target_window(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tab_entity, tab_id, _guard) = restored_remote_tab(this, cx);
                point_at_unreachable_host(&tab_entity, cx);
                let data = this
                    .extract_tab_transfer_data(tab_id, cx)
                    .expect("expected transfer data");
                this.pending_tab_transfer = Some(data);

                this.handle_pending_tab_transfer(window, cx);

                let received_id = this.active_tab_id.expect("expected the received tab");
                assert_ne!(received_id, tab_id);
                let editor = this.get_active_editor_tab(cx).expect("expected an editor");
                assert!(editor.awaiting_remote_content);
                assert!(!editor.content.read(cx).is_editable());
                assert!(this.inflight_remote_restore.contains(&received_id));
            });
        });
    }
}
