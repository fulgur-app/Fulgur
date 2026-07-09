use super::{RemoteFileResult, RemoteOpenResult};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{Fulgur, editor_tab, tab::Tab, ui::menus::build_menus};
use gpui::{Context, Window};
use gpui_component::{WindowExt, notification::NotificationType};
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
                        self.pending_remote_restore.remove(&tab_id);
                        self.apply_remote_reload_to_existing_tab(tab_id, remote_file, window, cx);
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
                                gpui::SharedString::from(
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
                        (NotificationType::Error, gpui::SharedString::from(msg)),
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
    /// - `remote_file`: Loaded remote payload from SSH worker
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_remote_reload_to_existing_tab(
        &mut self,
        tab_id: TabId,
        remote_file: RemoteFileResult,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(editor_tab) = self.tabs.iter_mut().find_map(|tab| match tab {
            Tab::Editor(editor_tab) if editor_tab.id == tab_id => Some(editor_tab),
            _ => None,
        }) else {
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
        editor_tab.force_language(window, cx, language, &self.settings.editor_settings);
        cx.notify();
    }
}
