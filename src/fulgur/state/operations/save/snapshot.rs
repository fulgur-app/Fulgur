use crate::fulgur::{
    Fulgur,
    editor_tab::TabLocation,
    state::persistence::{
        SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState,
        format_modified_time,
    },
    tab::Tab,
    ui::components_utils::UNTITLED,
};
use gpui_kit::{App, Window};

impl Fulgur {
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
                // Shared with the close-flow warning, so a buffer this snapshot
                // leaves out for its size is always offered a save first.
                let keeps_buffer = editor_tab.snapshot_keeps_buffer(persist_unsaved, cx);
                let tab_state = match &editor_tab.location {
                    TabLocation::Local(path) => {
                        if keeps_buffer {
                            let current_content = editor_tab.content.read(cx).text().clone();
                            // A missing file leaves the buffer as the only copy, even
                            // when no watch event reported the deletion.
                            let last_saved = editor_tab
                                .file_last_modified
                                .filter(|_| path.exists())
                                .and_then(format_modified_time);
                            TabState {
                                tab_id: editor_tab.id.0,
                                title: editor_tab.title.to_string(),
                                log_view: editor_tab.log_view,
                                color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                                file_path: Some(path.clone()),
                                content: Some(TabContent::Rope(current_content)),
                                last_saved,
                                remote: None,
                                share: None,
                                encoding: Some(editor_tab.encoding.clone()),
                                lossy_decode: editor_tab.lossy_decode,
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
                                share: None,
                                encoding: Some(editor_tab.encoding.clone()),
                                lossy_decode: editor_tab.lossy_decode,
                            }
                        }
                    }
                    TabLocation::Remote(remote_spec) => {
                        let content = keeps_buffer
                            .then(|| TabContent::Rope(editor_tab.content.read(cx).text().clone()));
                        TabState {
                            tab_id: editor_tab.id.0,
                            title: editor_tab.title.to_string(),
                            log_view: editor_tab.log_view,
                            color_tag: editor_tab.color_tag.map(|c| c.key().to_string()),
                            file_path: None,
                            content,
                            last_saved: None,
                            remote: Some(SerializedRemoteSpec::from_remote_spec(remote_spec)),
                            share: None,
                            encoding: Some(editor_tab.encoding.clone()),
                            lossy_decode: editor_tab.lossy_decode,
                        }
                    }
                    TabLocation::Untitled | TabLocation::Shared(_) => {
                        if !keeps_buffer {
                            log::debug!(
                                "Not persisting untitled tab '{}': unsaved buffer persistence is disabled or the content exceeds the large-file threshold",
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
                            share: editor_tab.location.share_origin().cloned(),
                            encoding: Some(editor_tab.encoding.clone()),
                            lossy_decode: editor_tab.lossy_decode,
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
    /// - `tab_states`: The editor tabs that will actually be persisted
    ///
    /// ### Returns
    /// - `Some(usize)`: the active editor tab index
    /// - `None`: if the active tab is Settings or its editor is not persisted
    fn active_editor_index_for_state(&self, cx: &App, tab_states: &[TabState]) -> Option<usize> {
        let active = self.active_tab_index(cx)?;
        let active_tab = self.tabs.get(active)?.read(cx);
        let editor_tab_id = match active_tab {
            Tab::Editor(et) => et.id,
            Tab::MarkdownPreview(pt) => pt.source_tab_id,
            Tab::Settings(_) => return None,
        };
        tab_states
            .iter()
            .position(|tab_state| tab_state.tab_id == editor_tab_id.0)
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
        let tabs = self.build_tab_states(cx);
        let active_tab_index = self.active_editor_index_for_state(cx, &tabs);
        WindowState {
            window_id: self.persistent_window_id,
            tabs,
            active_tab_index,
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
        let tabs = self.build_tab_states(cx);
        let active_tab_index = self.active_editor_index_for_state(cx, &tabs);
        WindowState {
            window_id: self.persistent_window_id,
            tabs,
            active_tab_index,
            window_bounds,
        }
    }
}
