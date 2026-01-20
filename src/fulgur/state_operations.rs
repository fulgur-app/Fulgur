use crate::fulgur::{
    Fulgur,
    editor_tab::EditorTab,
    files::file_operations::detect_encoding_and_decode,
    state_persistence::*,
    tab::Tab,
    ui::{
        components_utils::{UNTITLED, UTF_8},
        languages::SupportedLanguage,
    },
};
use gpui::*;
use gpui_component::{highlighter::Language, input::TabSize};
use std::fs;

impl Fulgur {
    /// Save the current app state to disk (saves all windows in multi-window mode)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_state(&self, cx: &App) -> anyhow::Result<()> {
        log::debug!("Saving application state...");
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        let current_window_id = self.window_id;
        let all_window_ids = window_manager.get_all_window_ids();
        for window_id in all_window_ids {
            if window_id == current_window_id {
                windows_state.windows.push(self.build_window_state(cx));
            } else {
                if let Some(weak_entity) = window_manager.get_window(window_id) {
                    if let Some(entity) = weak_entity.upgrade() {
                        windows_state
                            .windows
                            .push(entity.read(cx).build_window_state(cx));
                    }
                }
            }
        }
        windows_state.save()?;
        log::debug!(
            "Application state saved successfully ({} windows, {} tabs in this window)",
            windows_state.windows.len(),
            self.tabs.len()
        );
        Ok(())
    }

    /// Load app state from disk and restore tabs
    ///
    /// ### Arguments
    /// - `window`: The window to load the state from
    /// - `cx`: The application context
    /// - `window_index`: Index of the window state to restore (0 = first window, etc.)
    pub fn load_state(&mut self, window: &mut Window, cx: &mut Context<Self>, window_index: usize) {
        log::debug!("Loading application state for window {}...", window_index);
        // Temporarily disable indent guides during restoration to prevent crash
        let original_indent_guides = self.settings.editor_settings.show_indent_guides;
        self.settings.editor_settings.show_indent_guides = false;
        if let Ok(windows_state) = WindowsState::load() {
            if let Some(window_state) = windows_state.windows.get(window_index) {
                log::debug!(
                    "State loaded successfully, restoring {} tabs",
                    window_state.tabs.len()
                );
                self.tabs.clear();
                for tab_state in &window_state.tabs {
                    let tab = self.restore_tab_from_state(tab_state.clone(), window, cx);
                    if let Some(editor_tab) = tab {
                        self.tabs.push(Tab::Editor(editor_tab));
                    }
                }
                if let Some(index) = window_state.active_tab_index {
                    if index < self.tabs.len() {
                        self.active_tab_index = Some(index);
                    } else if !self.tabs.is_empty() {
                        self.active_tab_index = Some(0);
                    } else {
                        self.active_tab_index = None;
                    }
                }
                self.next_tab_id = window_state.next_tab_id;
                cx.notify();
            }
        } else {
            log::warn!("Failed to load application state, starting fresh");
        }
        if self.tabs.is_empty() {
            log::debug!("No tabs restored, creating initial empty tab");
            let initial_tab = Tab::Editor(EditorTab::new(
                0,
                UNTITLED,
                window,
                cx,
                &self.settings.editor_settings,
            ));
            self.tabs.push(initial_tab);
            self.active_tab_index = Some(0);
            self.next_tab_id = 1;
        }
        self.settings.editor_settings.show_indent_guides = original_indent_guides;
        if original_indent_guides {
            self.settings_changed = true;
        }
    }

    /// Restore a single tab from saved state:
    ///
    /// - If a file exists, it will be loaded from the file.
    /// - If a file exists and was modified but unsaved in the last state save and not modified after externally, it'll be loaded from the saved state
    /// - If a file does not exist, the saved content will be used.
    /// - If no path and no content is provided, the tab will be skipped.
    ///
    /// ### Arguments
    /// - `tab_state`: The saved state of the tab
    /// - `window`: The window to restore the tab to
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(EditorTab)`: The restored tab
    /// - `None`: If the tab could not be restored
    fn restore_tab_from_state(
        &self,
        tab_state: TabState,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<EditorTab> {
        log::debug!("Restoring tab: {}", tab_state.title);
        let is_modified = tab_state.content.is_some();
        let (content, path, encoding) = if let Some(saved_path) = tab_state.file_path {
            if let Some(saved_content) = tab_state.content {
                if saved_path.exists() {
                    if let Some(ref saved_time) = tab_state.last_saved {
                        if let Some(file_time) = get_file_modified_time(&saved_path) {
                            if is_file_newer(&file_time, saved_time) {
                                if let Ok(bytes) = fs::read(&saved_path) {
                                    let (enc, file_content) = detect_encoding_and_decode(&bytes);
                                    (file_content, Some(saved_path), enc)
                                } else {
                                    (saved_content, Some(saved_path), UTF_8.to_string())
                                }
                            } else {
                                (saved_content, Some(saved_path), UTF_8.to_string())
                            }
                        } else {
                            (saved_content, Some(saved_path), UTF_8.to_string())
                        }
                    } else {
                        (saved_content, Some(saved_path), UTF_8.to_string())
                    }
                } else {
                    (saved_content, None, UTF_8.to_string())
                }
            } else {
                if saved_path.exists() {
                    if let Ok(bytes) = fs::read(&saved_path) {
                        let (enc, file_content) = detect_encoding_and_decode(&bytes);
                        (file_content, Some(saved_path), enc)
                    } else {
                        return None;
                    }
                } else {
                    return None;
                }
            }
        } else {
            if let Some(saved_content) = tab_state.content {
                (saved_content, None, UTF_8.to_string())
            } else {
                return None;
            }
        };
        let tab = if let Some(file_path) = path {
            EditorTab::from_file(
                tab_state.id,
                file_path,
                content,
                encoding,
                window,
                cx,
                &self.settings.editor_settings,
                is_modified,
            )
        } else {
            let content_entity = cx.new(|cx| {
                gpui_component::input::InputState::new(window, cx)
                    .code_editor(Language::Plain.name())
                    .line_number(self.settings.editor_settings.show_line_numbers)
                    .indent_guides(self.settings.editor_settings.show_indent_guides)
                    .tab_size(TabSize {
                        tab_size: self.settings.editor_settings.tab_size,
                        hard_tabs: false,
                    })
                    .soft_wrap(self.settings.editor_settings.soft_wrap)
                    .default_value(content)
            });
            EditorTab {
                id: tab_state.id,
                title: tab_state.title.into(),
                content: content_entity,
                file_path: None,
                modified: true,
                original_content: String::new(),
                encoding: "UTF-8".to_string(),
                language: SupportedLanguage::Plain,
                show_markdown_toolbar: self
                    .settings
                    .editor_settings
                    .markdown_settings
                    .show_markdown_toolbar,
                show_markdown_preview: self
                    .settings
                    .editor_settings
                    .markdown_settings
                    .show_markdown_preview,
            }
        };

        Some(tab)
    }

    /// Build WindowState for this window (helper for save_all_windows_state)
    ///
    /// ### Arguments
    /// - `cx`: The application context.
    ///
    /// ### Returns
    /// - `WindowState`: The WindowState for this window.
    pub fn build_window_state(&self, cx: &App) -> WindowState {
        let mut tab_states = Vec::new();
        for tab in &self.tabs {
            if let Some(editor_tab) = tab.as_editor() {
                let current_content = editor_tab.content.read(cx).text().to_string();
                let is_modified = current_content != editor_tab.original_content;
                if editor_tab.file_path.is_none() && current_content.is_empty() {
                    continue;
                }
                let tab_state = if let Some(ref path) = editor_tab.file_path {
                    if is_modified {
                        TabState {
                            id: editor_tab.id,
                            title: editor_tab.title.to_string(),
                            file_path: Some(path.clone()),
                            content: Some(current_content),
                            last_saved: get_file_modified_time(path),
                        }
                    } else {
                        TabState {
                            id: editor_tab.id,
                            title: editor_tab.title.to_string(),
                            file_path: Some(path.clone()),
                            content: None,
                            last_saved: None,
                        }
                    }
                } else {
                    TabState {
                        id: editor_tab.id,
                        title: editor_tab.title.to_string(),
                        file_path: None,
                        content: Some(current_content),
                        last_saved: None,
                    }
                };
                tab_states.push(tab_state);
            }
        }
        WindowState {
            tabs: tab_states,
            active_tab_index: self.active_tab_index,
            next_tab_id: self.next_tab_id,
        }
    }

    /// Save all windows' state to disk
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Ok(())`: If all windows' state was saved successfully
    /// - `Err(anyhow::Error)`: If the state could not be saved
    #[allow(dead_code)]
    pub fn save_all_windows_state(cx: &mut App) -> anyhow::Result<()> {
        log::debug!("Saving all windows state...");
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        for weak_entity in window_manager.get_all_windows() {
            if let Some(entity) = weak_entity.upgrade() {
                windows_state
                    .windows
                    .push(entity.read(cx).build_window_state(cx));
            }
        }
        windows_state.save()?;
        log::debug!(
            "All windows state saved successfully ({} windows)",
            windows_state.windows.len()
        );
        Ok(())
    }
}
