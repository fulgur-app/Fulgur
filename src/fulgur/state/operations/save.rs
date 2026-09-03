use super::super::persistence::{
    SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
    get_file_modified_time,
};
use crate::fulgur::{Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED};
use gpui::{App, Window};

impl Fulgur {
    /// Save the current app state to disk (saves all windows in multi-window mode)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    ///
    /// ### Errors
    /// - Returns an error if the state cannot be persisted (no state database
    ///   available, or the transaction failed).
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_state(&self, cx: &App, window: &Window) -> anyhow::Result<()> {
        log::debug!("Saving application state...");
        let windows_state = self.build_windows_state(cx, window);
        let window_count = windows_state.windows.len();
        let tab_count = self.tabs.len();
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_blocking(windows_state)?;
        log::debug!(
            "Application state saved successfully ({window_count} windows, {tab_count} tabs in this window)"
        );
        Ok(())
    }

    /// Save the current app state to disk without blocking the UI thread.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    pub fn save_state_async(&self, cx: &App, window: &Window) {
        log::debug!("Saving application state (async)...");
        let windows_state = self.build_windows_state(cx, window);
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_async(windows_state);
    }

    /// Assemble the full multi-window state snapshot for persistence.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The current window (needed for its bounds)
    ///
    /// ### Returns
    /// - `WindowsState`: The snapshot of all open windows
    fn build_windows_state(&self, cx: &App, window: &Window) -> WindowsState {
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        let current_window_id = self.window_id;
        let all_window_ids = window_manager.get_all_window_ids();
        for window_id in &all_window_ids {
            if *window_id == current_window_id {
                windows_state
                    .windows
                    .push(self.build_window_state(cx, window));
            } else if let Some(weak_entity) = window_manager.get_window(*window_id)
                && let Some(entity) = weak_entity.upgrade()
            {
                windows_state
                    .windows
                    .push(entity.read(cx).build_window_state_without_bounds(cx));
            }
        }
        windows_state
    }

    /// Build tab states for all tabs in this window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<TabState>`: The tab states for all tabs
    fn build_tab_states(&self, cx: &App) -> Vec<TabState> {
        let persist_unsaved = self.settings.app_settings.persist_unsaved_buffers;
        let mut tab_states = Vec::new();
        for tab in &self.tabs {
            if let Some(editor_tab) = tab.read(cx).as_editor() {
                let tab_state = match &editor_tab.location {
                    TabLocation::Local(path) => {
                        if persist_unsaved
                            && editor_tab.content_differs_from_original(cx)
                            && !editor_tab.content_too_large_to_persist(cx)
                        {
                            let current_content = editor_tab.content.read(cx).text().clone();
                            TabState {
                                tab_id: editor_tab.id.0,
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                                file_path: Some(path.clone()),
                                content: Some(TabContent::Rope(current_content)),
                                last_saved: get_file_modified_time(path),
                                remote: None,
                            }
                        } else {
                            TabState {
                                tab_id: editor_tab.id.0,
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                                file_path: Some(path.clone()),
                                content: None,
                                last_saved: None,
                                remote: None,
                            }
                        }
                    }
                    TabLocation::Remote(remote_spec) => {
                        let content = if persist_unsaved
                            && editor_tab.content_differs_from_original(cx)
                            && !editor_tab.content_too_large_to_persist(cx)
                        {
                            Some(TabContent::Rope(editor_tab.content.read(cx).text().clone()))
                        } else {
                            None
                        };
                        TabState {
                            tab_id: editor_tab.id.0,
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
                            color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                            file_path: None,
                            content,
                            last_saved: None,
                            remote: Some(SerializedRemoteSpec::from_remote_spec(remote_spec)),
                        }
                    }
                    TabLocation::Untitled => {
                        if !persist_unsaved {
                            log::debug!(
                                "Not persisting untitled tab '{}': unsaved buffer persistence is disabled",
                                editor_tab.title
                            );
                            continue;
                        }
                        if editor_tab.content_too_large_to_persist(cx) {
                            log::warn!(
                                "Not persisting untitled tab '{}': content exceeds the large-file threshold",
                                editor_tab.title
                            );
                            continue;
                        }
                        let current_content =
                            TabContent::Rope(editor_tab.content.read(cx).text().clone());
                        if current_content.is_empty() && editor_tab.title.starts_with(UNTITLED) {
                            continue;
                        }
                        TabState {
                            tab_id: editor_tab.id.0,
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
                            color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                            file_path: None,
                            content: Some(current_content),
                            last_saved: None,
                            remote: None,
                        }
                    }
                };
                tab_states.push(tab_state);
            }
        }
        tab_states
    }

    /// Compute the active tab index relative to the editor-only tab list for state persistence.
    ///
    /// Preview tabs are not saved, so the persisted active index must refer to an editor tab.
    /// If the active tab is a preview tab, the index of its source editor tab is returned.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(usize)`: the active editor tab index
    /// - `None`: if the active tab is a Settings tab (not persisted).
    fn active_editor_index_for_state(&self, cx: &App) -> Option<usize> {
        let active = self.active_tab_index(cx)?;
        let active_tab = self.tabs.get(active)?.read(cx);
        let editor_tab_id = match active_tab {
            Tab::Editor(et) => et.id,
            Tab::MarkdownPreview(pt) => pt.source_tab_id,
            Tab::Settings(_) => return None,
        };
        let mut editor_index = 0;
        for tab in &self.tabs {
            if let Tab::Editor(et) = tab.read(cx) {
                if et.id == editor_tab_id {
                    return Some(editor_index);
                }
                editor_index += 1;
            }
        }
        None
    }

    /// Build `WindowState` for this window without window bounds (for cross-window saves)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `WindowState`: The `WindowState` for this window (with cached bounds)
    pub fn build_window_state_without_bounds(&self, cx: &App) -> WindowState {
        let window_bounds = self.cached_window_bounds.clone().unwrap_or_default();
        WindowState {
            window_id: self.persistent_window_id,
            tabs: self.build_tab_states(cx),
            active_tab_index: self.active_editor_index_for_state(cx),
            window_bounds,
        }
    }

    /// Build `WindowState` for this window (with window bounds)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window (needed for bounds)
    ///
    /// ### Returns
    /// - `WindowState`: The `WindowState` for this window
    pub fn build_window_state(&self, cx: &App, window: &Window) -> WindowState {
        let display_id = window
            .display(cx)
            .and_then(|d| u32::try_from(u64::from(d.id())).ok());
        let window_bounds =
            SerializedWindowBounds::from_gpui_bounds(window.window_bounds(), display_id);
        WindowState {
            window_id: self.persistent_window_id,
            tabs: self.build_tab_states(cx),
            active_tab_index: self.active_editor_index_for_state(cx),
            window_bounds,
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{Fulgur, editor_tab::TabLocation, state::persistence::TabState};
    use gpui::{Entity, TestAppContext, VisualTestContext};

    use crate::fulgur::test_support::setup_fulgur_with_root as setup_fulgur;
    /// Give the first tab a location and some dirty content, then snapshot the window.
    fn tab_states_with(
        fulgur: &Entity<Fulgur>,
        cx: &mut VisualTestContext,
        location: TabLocation,
        persist_unsaved_buffers: bool,
    ) -> Vec<TabState> {
        cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.settings.app_settings.persist_unsaved_buffers = persist_unsaved_buffers;
                let tab = this
                    .tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone();
                tab.update(cx, |tab, cx| {
                    if let Some(editor_tab) = tab.as_editor_mut() {
                        editor_tab.location = location;
                        editor_tab.content.update(cx, |content, cx| {
                            content.set_value("dirty content", window, cx);
                        });
                    }
                });
            });
        });
        fulgur.read_with(cx, |this, cx| {
            this.build_window_state_without_bounds(cx).tabs
        })
    }

    #[gpui::test]
    fn dirty_file_tab_persists_content_when_setting_is_enabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(
            &fulgur,
            &mut visual_cx,
            TabLocation::Local("/tmp/notes.txt".into()),
            true,
        );
        assert_eq!(tabs.len(), 1);
        assert!(
            tabs[0].content.is_some(),
            "unsaved content must be persisted when the setting is enabled"
        );
    }

    #[gpui::test]
    fn dirty_file_tab_persists_path_only_when_setting_is_disabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(
            &fulgur,
            &mut visual_cx,
            TabLocation::Local("/tmp/notes.txt".into()),
            false,
        );
        assert_eq!(tabs.len(), 1, "the tab itself must still be restored");
        assert!(
            tabs[0].content.is_none(),
            "unsaved content must not be persisted when the setting is disabled"
        );
        assert!(tabs[0].file_path.is_some());
    }

    #[gpui::test]
    fn untitled_tab_is_dropped_when_setting_is_disabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, false);
        assert!(
            tabs.is_empty(),
            "an untitled tab carries nothing but its unsaved content, so it must be dropped"
        );
    }

    #[gpui::test]
    fn untitled_tab_is_persisted_when_setting_is_enabled(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let tabs = tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, true);
        assert_eq!(tabs.len(), 1);
        assert!(tabs[0].content.is_some());
    }
}
