use super::super::persistence::{TabState, get_file_modified_time};
use super::decision::{TabRestoreDecision, determine_tab_restore_strategy};
use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams, TabLocation},
    files::file_operations::{RemoteFileResult, detect_encoding_and_decode},
    languages::supported_languages::{
        SupportedLanguage, language_from_content, language_registry_name,
    },
    tab::{Tab, TabId},
    ui::components_utils::{UNTITLED, UTF_8},
    ui::tabs::color_tag::ColorTag,
};
use gpui::{App, AppContext, Context, Window};
use gpui_component::input::TabSize;
use std::fs;
use std::io::Read;

impl Fulgur {
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
        let window_state = cx
            .global::<crate::fulgur::shared_state::SharedAppState>()
            .restore_state
            .lock()
            .as_ref()
            .and_then(|ws| ws.windows.get(window_index).cloned());
        if let Some(window_state) = window_state {
            log::debug!(
                "Restoring {} tabs from startup snapshot",
                window_state.tabs.len()
            );
            self.tabs.clear();
            self.pending_remote_restore.clear();
            self.inflight_remote_restore.clear();
            let mut tab_id = TabId(0);
            for tab_state in &window_state.tabs {
                let tab = self.restore_tab_from_state(tab_state.clone(), tab_id, window, cx);
                if let Some(editor_tab) = tab {
                    self.tabs.push(Tab::Editor(editor_tab).into_entity(cx));
                    tab_id = tab_id.next();
                }
            }
            let saved_active_editor_id: Option<TabId> = window_state
                .active_tab_index
                .and_then(|idx| self.tabs.get(idx))
                .and_then(|t| t.read(cx).as_editor())
                .map(|et| et.id);
            self.next_tab_id = tab_id;
            self.insert_preview_tabs_for_markdown(cx);
            self.active_tab_id =
                saved_active_editor_id.or_else(|| self.tabs.first().map(|t| t.read(cx).id()));

            let active_log_tab_id = self
                .active_tab(cx)
                .and_then(Tab::as_editor)
                .filter(|editor| editor.log_view)
                .map(|editor| editor.id);
            if let Some(active_id) = active_log_tab_id {
                self.activate_log_view(active_id, window, cx);
            }

            cx.notify();
        } else {
            log::warn!("No saved state for window {window_index}, starting fresh");
        }
        if self.tabs.is_empty() {
            log::debug!("No tabs restored, creating initial empty tab");
            let initial_tab = Tab::Editor(EditorTab::new(
                TabId(0),
                UNTITLED,
                window,
                cx,
                &self.settings.editor_settings,
            ));
            self.active_tab_id = Some(initial_tab.id());
            self.tabs.push(initial_tab.into_entity(cx));
            self.next_tab_id = TabId(1);
        }
        self.settings.editor_settings.show_indent_guides = original_indent_guides;
        if original_indent_guides {
            // Indent guides were disabled during restoration; push the real
            // settings into every restored tab once this update cycle is done.
            cx.defer_in(window, |this, window, cx| {
                this.apply_editor_settings_to_tabs(window, cx);
            });
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
        tab_id: TabId,
        window: &mut Window,
        cx: &mut App,
    ) -> Option<EditorTab> {
        log::debug!("Restoring tab: {}", tab_state.title);

        let color_tag = tab_state.color_tag.as_deref().and_then(ColorTag::from_key);
        let file_exists = tab_state.file_path.as_ref().is_some_and(|p| p.exists());
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
        let (content, path, encoding, is_modified, lossy_decode) = match decision {
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
                        lossy: false,
                    },
                    window,
                    cx,
                    &self.settings.editor_settings,
                );
                tab.modified = is_modified;
                tab.color_tag = color_tag;
                self.pending_remote_restore.insert(tab_id);
                return Some(tab);
            }
            TabRestoreDecision::LoadFromFile { path } => {
                let mut bytes = Vec::new();
                let mut file = readable_file.take()?;
                file.read_to_end(&mut bytes).ok()?;
                let decoded = detect_encoding_and_decode(bytes);
                (
                    decoded.content,
                    Some(path),
                    decoded.encoding,
                    false,
                    decoded.lossy,
                )
            }
            TabRestoreDecision::UseSavedContentWithPath { path, content } => {
                (content, Some(path), UTF_8.to_string(), true, false)
            }
            TabRestoreDecision::UseSavedContentNoPath { content } => {
                (content, None, UTF_8.to_string(), true, false)
            }
            TabRestoreDecision::Skip => return None,
        };
        let mut tab = if let Some(file_path) = path {
            let mut tab = EditorTab::from_file(
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
            );
            tab.lossy_decode = lossy_decode;
            tab
        } else {
            let language = language_from_content(&tab_state.title, &content);
            let large_file = crate::fulgur::ui::tabs::editor_tab::is_large_file(content.len());
            // In large-file mode, substitute a language with no registered grammar so the background tree-sitter parser never runs.
            let language_name = if large_file {
                language_registry_name(&SupportedLanguage::Plain)
            } else {
                language_registry_name(&language)
            };
            let (csv_view_mode, csv_delimiter) =
                crate::fulgur::ui::tabs::editor_tab::initial_csv_state(language, &content);
            let content_entity = cx.new(|cx| {
                gpui_component::input::InputState::new(window, cx)
                    .code_editor(language_name)
                    .line_number(self.settings.editor_settings.show_line_numbers)
                    .indent_guides(self.settings.editor_settings.show_indent_guides)
                    .tab_size(TabSize {
                        tab_size: self.settings.editor_settings.tab_size,
                        hard_tabs: false,
                    })
                    .soft_wrap(self.settings.editor_settings.soft_wrap && !large_file)
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
                lossy_decode: false,
                language,
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
                large_file,
                csv_view_mode,
                csv_delimiter,
                csv_table: None,
                csv_table_source_hash: 0,
                color_tag: None,
                log_view: false,
                log_follow: true,
                log_full: false,
                log_content: None,
                content_subscription: None,
            }
        };

        let opens_as_log = tab
            .file_path()
            .is_some_and(|path| crate::fulgur::ui::log_view::opens_as_log_by_default(path));
        if tab_state.log_view || opens_as_log {
            tab.log_view = true;
        }
        tab.color_tag = color_tag;
        Some(tab)
    }
}
