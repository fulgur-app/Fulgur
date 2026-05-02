use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams, TabLocation},
    files::file_operations::{RemoteFileResult, detect_encoding_and_decode},
    languages::supported_languages::SupportedLanguage,
    state_persistence::{
        SerializedRemoteSpec, SerializedWindowBounds, TabState, WindowState, WindowsState,
        get_file_modified_time, is_file_newer,
    },
    tab::Tab,
    ui::components_utils::{UNTITLED, UTF_8},
};
use gpui::{App, AppContext, Context, Window};
use gpui_component::{highlighter::Language, input::TabSize};
use std::fs;
use std::io::Read;
use std::path::PathBuf;

/// Decision for how to restore a tab from saved state
#[derive(Debug, PartialEq, Eq)]
pub enum TabRestoreDecision {
    /// Restore a remote tab (SSH/SFTP) from serialized metadata.
    RestoreRemote {
        remote: SerializedRemoteSpec,
        content: Option<String>,
    },
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
    saved_remote: Option<SerializedRemoteSpec>,
    saved_content: Option<String>,
    last_saved: Option<String>,
    file_exists: bool,
    file_modified_time: Option<String>,
    can_read_file: bool,
) -> TabRestoreDecision {
    if let Some(remote) = saved_remote {
        return TabRestoreDecision::RestoreRemote {
            remote,
            content: saved_content,
        };
    }

    match (saved_path, saved_content) {
        // Case 1: Has both path and content (modified file)
        (Some(path), Some(content)) => {
            if file_exists {
                if let (Some(ref saved_time), Some(ref file_time)) =
                    (last_saved, file_modified_time)
                {
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

    /// Load app state from disk and restore tabs
    ///
    /// ### Arguments
    /// - `window`: The window to load the state from
    /// - `cx`: The application context
    /// - `window_index`: Index of the window state to restore (0 = first window, etc.)
    pub fn load_state(&mut self, window: &mut Window, cx: &mut Context<Self>, window_index: usize) {
        log::debug!("Loading application state for window {window_index}...");
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
                self.pending_remote_restore.clear();
                self.inflight_remote_restore.clear();
                let mut tab_id = 0;
                for tab_state in &window_state.tabs {
                    let tab = self.restore_tab_from_state(tab_state.clone(), tab_id, window, cx);
                    if let Some(editor_tab) = tab {
                        self.tabs.push(Tab::Editor(editor_tab));
                        tab_id += 1;
                    }
                }
                let saved_active_editor_id: Option<usize> = window_state
                    .active_tab_index
                    .and_then(|idx| self.tabs.get(idx))
                    .and_then(|t| t.as_editor())
                    .map(|et| et.id);
                self.next_tab_id = tab_id;
                self.insert_preview_tabs_for_markdown();
                self.active_tab_index = if let Some(target_id) = saved_active_editor_id {
                    self.tabs
                        .iter()
                        .position(|t| t.id() == target_id)
                        .or(if !self.tabs.is_empty() { Some(0) } else { None })
                } else if !self.tabs.is_empty() {
                    Some(0)
                } else {
                    None
                };

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
        &mut self,
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
        let mut readable_file = tab_state
            .file_path
            .as_ref()
            .and_then(|path| fs::File::open(path).ok());
        let can_read_file = readable_file.is_some();
        let decision = determine_tab_restore_strategy(
            tab_state.file_path.clone(),
            tab_state.remote.clone(),
            tab_state.content.clone(),
            tab_state.last_saved,
            file_exists,
            file_modified_time,
            can_read_file,
        );
        let (content, path, encoding, is_modified) = match decision {
            TabRestoreDecision::RestoreRemote { remote, content } => {
                let is_modified = content.is_some();
                let restored_content = content.unwrap_or_default();
                let mut tab = EditorTab::from_remote_loaded(
                    tab_id,
                    RemoteFileResult {
                        spec: remote.to_remote_spec(),
                        file_size: restored_content.len(),
                        content: restored_content,
                        encoding: UTF_8.to_string(),
                    },
                    window,
                    cx,
                    &self.settings.editor_settings,
                );
                tab.modified = is_modified;
                self.pending_remote_restore.insert(tab_id);
                return Some(tab);
            }
            TabRestoreDecision::LoadFromFile { path } => {
                let mut bytes = Vec::new();
                let mut file = readable_file.take()?;
                file.read_to_end(&mut bytes).ok()?;
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
                    .show_whitespaces(self.settings.editor_settings.show_whitespaces)
                    .default_value(content)
            });
            EditorTab {
                id: tab_id,
                title: tab_state.title.into(),
                content: content_entity,
                location: TabLocation::Untitled,
                modified: true,
                original_content_hash:
                    crate::fulgur::ui::tabs::editor_tab::content_fingerprint_from_str("").0,
                original_content_len: 0,
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
                file_size_bytes: None,
                file_last_modified: None,
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
                let tab_state = match &editor_tab.location {
                    TabLocation::Local(path) => {
                        if editor_tab.content_differs_from_original(cx) {
                            let current_content = editor_tab.content.read(cx).text().to_string();
                            TabState {
                                title: editor_tab.title.to_string(),
                                file_path: Some(path.clone()),
                                content: Some(current_content),
                                last_saved: get_file_modified_time(path),
                                remote: None,
                            }
                        } else {
                            TabState {
                                title: editor_tab.title.to_string(),
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
    /// ### Returns
    /// - `Some(usize)`: the active editor tab index
    /// - `None`: if the active tab is a Settings tab (not persisted).
    fn active_editor_index_for_state(&self) -> Option<usize> {
        let active = self.active_tab_index?;
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
            active_tab_index: self.active_editor_index_for_state(),
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
            active_tab_index: self.active_editor_index_for_state(),
            window_bounds,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{TabRestoreDecision, determine_tab_restore_strategy};
    use crate::fulgur::state_persistence::SerializedRemoteSpec;
    use std::path::PathBuf;

    #[test]
    fn test_determine_tab_restore_strategy_loads_from_file_when_newer_and_readable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T09:00:00Z".to_string()),
            true,
            Some("2026-04-07T10:00:00Z".to_string()),
            true,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::LoadFromFile {
                path: PathBuf::from("/tmp/example.md")
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_uses_saved_content_when_newer_but_unreadable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T09:00:00Z".to_string()),
            true,
            Some("2026-04-07T10:00:00Z".to_string()),
            false,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::UseSavedContentWithPath {
                path: PathBuf::from("/tmp/example.md"),
                content: "saved".to_string(),
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_skips_path_only_tab_when_unreadable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            None,
            None,
            true,
            None,
            false,
        );
        assert_eq!(decision, TabRestoreDecision::Skip);
    }

    #[test]
    fn test_determine_tab_restore_strategy_loads_path_only_tab_when_readable() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            None,
            None,
            true,
            None,
            true,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::LoadFromFile {
                path: PathBuf::from("/tmp/example.md")
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_uses_saved_content_without_path_when_missing_file() {
        let decision = determine_tab_restore_strategy(
            Some(PathBuf::from("/tmp/example.md")),
            None,
            Some("saved".to_string()),
            Some("2026-04-07T09:00:00Z".to_string()),
            false,
            None,
            false,
        );
        assert_eq!(
            decision,
            TabRestoreDecision::UseSavedContentNoPath {
                content: "saved".to_string(),
            }
        );
    }

    #[test]
    fn test_determine_tab_restore_strategy_restores_remote_tabs() {
        let remote = SerializedRemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            path: "/tmp/test.txt".to_string(),
        };

        let decision = determine_tab_restore_strategy(
            None,
            Some(remote.clone()),
            Some("cached".to_string()),
            None,
            false,
            None,
            false,
        );

        assert_eq!(
            decision,
            TabRestoreDecision::RestoreRemote {
                remote,
                content: Some("cached".to_string()),
            }
        );
    }
}
