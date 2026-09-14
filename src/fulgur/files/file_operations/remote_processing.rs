use super::{RemoteFileResult, RemoteOpenResult, remote_types::RemoteReloadGuard};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{Fulgur, editor_tab, tab::Tab, ui::menus::build_menus};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{Context, Window};
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
                        if target_reload_guard.is_some_and(|guard| {
                            self.apply_remote_reload_to_existing_tab(
                                tab_id,
                                &guard,
                                remote_file,
                                window,
                                cx,
                            )
                        }) {
                            self.pending_remote_restore.remove(&tab_id);
                        }
                    } else {
                        self.last_failed_remote_open_url = None;
                        let recent_remote_url =
                            crate::fulgur::sync::ssh::url::format_remote_url(&remote_file.spec);
                        log::debug!(
                            "Remote file loaded: {}:{}",
                            remote_file.spec.host,
                            remote_file.spec.path
                        );
                        let new_tab_id = self.allocate_tab_id();
                        let editor_tab = editor_tab::EditorTab::from_remote_loaded(
                            new_tab_id,
                            remote_file,
                            window,
                            cx,
                            &self.settings.editor_settings,
                        );
                        self.place_editor_tab_reusing_scratch(Tab::Editor(editor_tab), window, cx);
                        self.focus_active_tab(window, cx);
                        if let Err(e) = self.settings.add_file(PathBuf::from(recent_remote_url)) {
                            log::error!("Failed to add remote file to recent files: {e}");
                        }
                        let update_link = Fulgur::shared_state(cx)
                            .update_info
                            .lock()
                            .as_ref()
                            .map(|info| info.download_url.clone());
                        let menus =
                            build_menus(&self.settings.get_recent_files(), update_link.as_deref());
                        self.update_menus(menus, cx);
                        self.save_state_async(cx, window);
                        cx.notify();
                    }
                }
                Ok(RemoteOpenResult::Browse(browse)) => {
                    if let Some(tab_id) = target_tab_id {
                        self.pending_remote_restore.insert(tab_id);
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
                        self.show_remote_path_browser_dialog(window, cx, &browse);
                    }
                }
                Err(msg) => {
                    if let Some(tab_id) = target_tab_id {
                        self.pending_remote_restore.insert(tab_id);
                    }
                    window.push_notification(
                        (NotificationType::Error, gpui_kit::SharedString::from(msg)),
                        cx,
                    );
                }
            }
        }
    }

    /// Apply fresh remote contents to an already-restored tab after lazy reconnect.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable editor tab id to update
    /// - `guard`: Content revision and source captured before the SSH read
    /// - `remote_file`: Loaded remote payload from SSH worker
    /// - `window`: The window context
    /// - `cx`: The application context
    /// ### Returns
    /// - `true`: The original tab and content revision still matched and were updated
    /// - `false`: The tab changed while the remote read was pending and was left untouched
    fn apply_remote_reload_to_existing_tab(
        &mut self,
        tab_id: TabId,
        guard: &RemoteReloadGuard,
        remote_file: RemoteFileResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let Some(tab_entity) = self.tab_entity_of(tab_id, cx) else {
            return false;
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
            return false;
        }
        tab_entity.update(cx, |tab, cx| {
            let Some(editor_tab) = tab.as_editor_mut() else {
                return;
            };
            editor_tab.content.update(cx, |input_state, cx| {
                input_state.set_value(&remote_file.content, window, cx);
            });
            editor_tab.location =
                crate::fulgur::editor_tab::TabLocation::Remote(remote_file.spec.clone());
            editor_tab.encoding = remote_file.encoding;
            editor_tab.set_original_content_from_str(&remote_file.content);
            editor_tab.modified = false;
            editor_tab.update_file_tooltip_cache(remote_file.file_size);
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
        true
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{RemoteFileResult, RemoteOpenResult};
    use crate::fulgur::{
        editor_tab::TabLocation,
        files::file_operations::{
            PendingRemoteOpenOutcome, remote_types::RemoteReloadGuard,
            test_helpers::setup_fulgur_with_root,
        },
        sync::ssh::url::RemoteSpec,
        ui::components_utils::UTF_8,
    };
    use gpui_kit::TestAppContext;

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
                    (editor.id, editor.content_revision(cx))
                });
                this.pending_remote_restore.insert(tab_id);

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
                    this.pending_remote_restore.contains(&tab_id),
                    "a rejected stale reload must not mark the restored tab as refreshed"
                );
            });
        });
    }
}
