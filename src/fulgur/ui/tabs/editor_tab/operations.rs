use super::{EditorTab, Jump};
use crate::fulgur::languages::supported_languages::{
    SupportedLanguage, language_from_content, language_registry_name,
};
use crate::fulgur::settings::EditorSettings;
use crate::fulgur::ui::components_utils::UNTITLED;
use gpui::{App, AppContext, Window};
use gpui_component::input::Position;
use std::time::SystemTime;

impl EditorTab {
    /// Update cached metadata used by tab tooltip rendering.
    ///
    /// ### Arguments
    /// - `content_len`: File size in bytes
    pub fn update_file_tooltip_cache(&mut self, content_len: usize) {
        self.file_size_bytes = Some(content_len as u64);
        self.file_last_modified = Some(SystemTime::now());
    }

    /// Update the editor's display settings. Tab size cannot be changed after InputState creation.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    pub fn update_settings(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) {
        let has_provider = self.content.read(cx).lsp.document_color_provider.is_some();
        let wants_provider = settings.highlight_colors;

        if has_provider != wants_provider {
            self.rebuild_input_state(window, cx, settings);
            return;
        }

        self.content.update(cx, |input_state, cx| {
            input_state.set_line_number(settings.show_line_numbers, window, cx);
            input_state.set_indent_guides(settings.show_indent_guides, window, cx);
            input_state.set_soft_wrap(settings.soft_wrap, window, cx);
            input_state.set_show_whitespaces(settings.show_whitespaces, window, cx);
        });
    }

    /// Rebuild the InputState to apply highlight_colors changes.
    ///
    /// The document_color_provider can only be set at creation time (Lsp internal
    /// state is not publicly clearable), so toggling the setting requires
    /// recreating the InputState while preserving cursor position.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `settings`: The editor settings
    pub fn rebuild_input_state(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) {
        let cursor = self.content.read(cx).cursor_position();
        let current_content = self.content.read(cx).text().to_string();
        self.content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&self.language),
                Some(current_content),
                settings,
            )
        });
        self.content.update(cx, |state, cx| {
            state.set_cursor_position(cursor, window, cx);
        });
    }

    /// Check if the tab's content has been modified
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `True` if the tab's content has been modified, `False` otherwise
    pub fn check_modified(&mut self, cx: &mut App) -> bool {
        let current_text = self.content.read(cx).text().to_string();
        self.modified = current_text != self.original_content;
        self.modified
    }

    /// Mark the tab as saved
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn mark_as_saved(&mut self, cx: &mut App) {
        self.original_content = self.content.read(cx).text().to_string();
        self.modified = false;
    }

    /// Get suggested filename for "Save as..." dialog
    ///
    /// ### Returns
    /// - `Some(String)`: The suggested filename
    /// - `None`: If the title is UNTITLED
    pub fn get_suggested_filename(&self) -> Option<String> {
        let title_str = self.title.to_string();
        let cleaned = title_str.trim_end_matches(" •").trim();
        if cleaned.is_empty() || cleaned.starts_with(UNTITLED) {
            None
        } else {
            Some(cleaned.to_string())
        }
    }

    /// Update the language/syntax highlighting based on the file extension
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `settings`: The editor settings for the new input state
    pub fn update_language(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) {
        if let Some(ref path) = self.file_path {
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let current_content = self.content.read(cx).text().to_string();
            let language = language_from_content(file_name, &current_content);
            self.force_language(window, cx, language, settings);
        }
    }

    /// Force the language/syntax highlighting based on the file extension.
    ///
    /// Recreates the `InputState` with the new language and restores the cursor position.
    /// Scroll state and undo history are not preserved.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `language`: The language to force
    /// - `settings`: The editor settings for the new input state
    pub fn force_language(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        language: SupportedLanguage,
        settings: &EditorSettings,
    ) {
        let cursor = self.content.read(cx).cursor_position();
        let current_content = self.content.read(cx).text().to_string();
        self.language = language;
        self.content = cx.new(|cx| {
            super::make_input_state(
                window,
                cx,
                language_registry_name(&language),
                Some(current_content),
                settings,
            )
        });
        self.content.update(cx, |state, cx| {
            state.set_cursor_position(cursor, window, cx);
        });
    }

    /// Jump to a specific line
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `jump`: The jump to perform
    pub fn jump_to_line(&mut self, window: &mut Window, cx: &mut App, jump: Jump) {
        self.content.update(cx, |input_state, cx| {
            input_state.set_cursor_position(
                Position {
                    line: jump.line,
                    character: jump.character.unwrap_or(0),
                },
                window,
                cx,
            );
            input_state.focus(window, cx);
            cx.notify();
        });
    }
}
