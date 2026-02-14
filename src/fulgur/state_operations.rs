use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams},
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
use std::path::PathBuf;

/// Decision for how to restore a tab from saved state
#[derive(Debug, PartialEq, Eq)]
pub enum TabRestoreDecision {
    /// Load content from file on disk
    LoadFromFile { path: PathBuf },
    /// Use saved content with file path
    UseSavedContentWithPath { path: PathBuf, content: String },
    /// Use saved content without file path (unsaved tab)
    UseSavedContentNoPath { content: String },
    /// Skip this tab (cannot be restored)
    Skip,
}

/// Determine how to restore a tab based on saved state and file system state
///
/// ### Arguments
/// - `saved_path`: The saved file path (if any)
/// - `saved_content`: The saved content (if any)
/// - `last_saved`: The last saved timestamp as ISO 8601 string (if any)
/// - `file_exists`: Whether the file exists on disk
/// - `file_modified_time`: The file's modification time as ISO 8601 string (if it exists)
/// - `can_read_file`: Whether the file can be read successfully
///
/// ### Returns
/// - `TabRestoreDecision`: The decision for how to restore this tab
pub fn determine_tab_restore_strategy(
    saved_path: Option<PathBuf>,
    saved_content: Option<String>,
    last_saved: Option<String>,
    file_exists: bool,
    file_modified_time: Option<String>,
    can_read_file: bool,
) -> TabRestoreDecision {
    match (saved_path, saved_content) {
        // Case 1: Has both path and content (modified file)
        (Some(path), Some(content)) => {
            if file_exists {
                if let (Some(ref saved_time), Some(ref file_time)) = (last_saved, file_modified_time) {
                    if is_file_newer(file_time, saved_time) {
                        if can_read_file {
                            TabRestoreDecision::LoadFromFile { path }
                        } else {
                            TabRestoreDecision::UseSavedContentWithPath { path, content }
                        }
                    } else {
                        TabRestoreDecision::UseSavedContentWithPath { path, content }
                    }
                } else {
                    TabRestoreDecision::UseSavedContentWithPath { path, content }
                }
            } else {
                TabRestoreDecision::UseSavedContentNoPath { content }
            }
        }
        (Some(path), None) => {
            if file_exists && can_read_file {
                TabRestoreDecision::LoadFromFile { path }
            } else {
                TabRestoreDecision::Skip
            }
        }
        (None, Some(content)) => TabRestoreDecision::UseSavedContentNoPath { content },
        (None, None) => TabRestoreDecision::Skip,
    }
}

impl Fulgur {
    /// Save the current app state to disk (saves all windows in multi-window mode)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_state(&self, cx: &App, window: &Window) -> anyhow::Result<()> {
        log::debug!("Saving application state...");
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        let current_window_id = self.window_id;
        let all_window_ids = window_manager.get_all_window_ids();
        for window_id in all_window_ids.iter() {
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
                let mut tab_id = 0;
                for tab_state in &window_state.tabs {
                    let tab = self.restore_tab_from_state(tab_state.clone(), tab_id, window, cx);
                    if let Some(editor_tab) = tab {
                        self.tabs.push(Tab::Editor(editor_tab));
                        tab_id += 1;
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
                self.next_tab_id = tab_id;
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
    /// - `tab_id`: The ID to assign to this tab (based on position)
    /// - `window`: The window to restore the tab to
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(EditorTab)`: The restored tab
    /// - `None`: If the tab could not be restored
    fn restore_tab_from_state(
        &self,
        tab_state: TabState,
        tab_id: usize,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<EditorTab> {
        log::debug!("Restoring tab: {}", tab_state.title);

        let file_exists = tab_state
            .file_path
            .as_ref()
            .map(|p| p.exists())
            .unwrap_or(false);
        let file_modified_time = tab_state
            .file_path
            .as_ref()
            .and_then(get_file_modified_time);
        let can_read_file = tab_state
            .file_path
            .as_ref()
            .map(|p| fs::read(p).is_ok())
            .unwrap_or(false);
        let decision = determine_tab_restore_strategy(
            tab_state.file_path.clone(),
            tab_state.content.clone(),
            tab_state.last_saved,
            file_exists,
            file_modified_time,
            can_read_file,
        );
        let (content, path, encoding, is_modified) = match decision {
            TabRestoreDecision::LoadFromFile { path } => {
                let bytes = fs::read(&path).ok()?;
                let (enc, file_content) = detect_encoding_and_decode(&bytes);
                (file_content, Some(path), enc, false)
            }
            TabRestoreDecision::UseSavedContentWithPath { path, content } => {
                (content, Some(path), UTF_8.to_string(), true)
            }
            TabRestoreDecision::UseSavedContentNoPath { content } => {
                (content, None, UTF_8.to_string(), true)
            }
            TabRestoreDecision::Skip => return None,
        };
        let tab = if let Some(file_path) = path {
            EditorTab::from_file(
                FromFileParams {
                    id: tab_id,
                    path: file_path,
                    contents: content,
                    encoding,
                    is_modified,
                },
                window,
                cx,
                &self.settings.editor_settings,
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
                id: tab_id,
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
                let current_content = editor_tab.content.read(cx).text().to_string();
                let is_modified = current_content != editor_tab.original_content;
                if editor_tab.file_path.is_none() && current_content.is_empty() {
                    continue;
                }
                let tab_state = if let Some(ref path) = editor_tab.file_path {
                    if is_modified {
                        TabState {
                            title: editor_tab.title.to_string(),
                            file_path: Some(path.clone()),
                            content: Some(current_content),
                            last_saved: get_file_modified_time(path),
                        }
                    } else {
                        TabState {
                            title: editor_tab.title.to_string(),
                            file_path: Some(path.clone()),
                            content: None,
                            last_saved: None,
                        }
                    }
                } else {
                    TabState {
                        title: editor_tab.title.to_string(),
                        file_path: None,
                        content: Some(current_content),
                        last_saved: None,
                    }
                };
                tab_states.push(tab_state);
            }
        }
        tab_states
    }

    /// Build WindowState for this window without window bounds (for cross-window saves)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `WindowState`: The WindowState for this window (with cached bounds)
    pub fn build_window_state_without_bounds(&self, cx: &App) -> WindowState {
        let window_bounds = self.cached_window_bounds.clone().unwrap_or_default();
        WindowState {
            tabs: self.build_tab_states(cx),
            active_tab_index: self.active_tab_index,
            window_bounds,
        }
    }

    /// Build WindowState for this window (with window bounds)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window (needed for bounds)
    ///
    /// ### Returns
    /// - `WindowState`: The WindowState for this window
    pub fn build_window_state(&self, cx: &App, window: &Window) -> WindowState {
        let display_id = window.display(cx).map(|d| d.id().into());
        let window_bounds =
            SerializedWindowBounds::from_gpui_bounds(window.window_bounds(), display_id);
        WindowState {
            tabs: self.build_tab_states(cx),
            active_tab_index: self.active_tab_index,
            window_bounds,
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
        for weak_entity in window_manager.get_all_windows().iter() {
            if let Some(entity) = weak_entity.upgrade() {
                // Each window has cached bounds that are updated in render
                windows_state
                    .windows
                    .push(entity.read(cx).build_window_state_without_bounds(cx));
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
