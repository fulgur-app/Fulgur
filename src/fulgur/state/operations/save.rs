use super::super::persistence::{
    SerializedRemoteSpec, SerializedWindowBounds, TabState, WindowState, WindowsState,
    get_file_modified_time,
};
use crate::fulgur::{Fulgur, editor_tab::TabLocation, tab::Tab};
use gpui::{App, Window};

impl Fulgur {
    /// Save the current app state to disk (saves all windows in multi-window mode)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    ///
    /// ### Errors
    /// - Returns an error if the state cannot be persisted to disk (path
    ///   resolution, serialization, or file write failure).
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_state(&self, cx: &App, window: &Window) -> anyhow::Result<()> {
        log::debug!("Saving application state...");
        let windows_state = self.build_windows_state(cx, window);
        let window_count = windows_state.windows.len();
        let tab_count = self.tabs.len();
        let path = WindowsState::state_file_path()?;
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_blocking(windows_state, path)?;
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
        let path = match WindowsState::state_file_path() {
            Ok(path) => path,
            Err(e) => {
                log::error!("Failed to resolve state file path for async save: {e}");
                return;
            }
        };
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_async(windows_state, path);
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
        let mut tab_states = Vec::new();
        for tab in &self.tabs {
            if let Some(editor_tab) = tab.as_editor() {
                let tab_state = match &editor_tab.location {
                    TabLocation::Local(path) => {
                        if editor_tab.content_differs_from_original(cx) {
                            let current_content = editor_tab.content.read(cx).text().to_string();
                            TabState {
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                file_path: Some(path.clone()),
                                content: Some(current_content),
                                last_saved: get_file_modified_time(path),
                                remote: None,
                            }
                        } else {
                            TabState {
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                file_path: Some(path.clone()),
                                content: None,
                                last_saved: None,
                                remote: None,
                            }
                        }
                    }
                    TabLocation::Remote(remote_spec) => {
                        let content = if editor_tab.content_differs_from_original(cx) {
                            Some(editor_tab.content.read(cx).text().to_string())
                        } else {
                            None
                        };
                        TabState {
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
                            file_path: None,
                            content,
                            last_saved: None,
                            remote: Some(SerializedRemoteSpec::from_remote_spec(remote_spec)),
                        }
                    }
                    TabLocation::Untitled => {
                        let current_content = editor_tab.content.read(cx).text().to_string();
                        if current_content.is_empty() {
                            continue;
                        }
                        TabState {
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
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
    /// ### Returns
    /// - `Some(usize)`: the active editor tab index
    /// - `None`: if the active tab is a Settings tab (not persisted).
    fn active_editor_index_for_state(&self) -> Option<usize> {
        let active = self.active_tab_index()?;
        let active_tab = self.tabs.get(active)?;
        let editor_tab_id = match active_tab {
            Tab::Editor(et) => et.id,
            Tab::MarkdownPreview(pt) => pt.source_tab_id,
            Tab::Settings(_) => return None,
        };
        let mut editor_index = 0;
        for tab in &self.tabs {
            if let Tab::Editor(et) = tab {
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
            tabs: self.build_tab_states(cx),
            active_tab_index: self.active_editor_index_for_state(),
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
            tabs: self.build_tab_states(cx),
            active_tab_index: self.active_editor_index_for_state(),
            window_bounds,
        }
    }
}
