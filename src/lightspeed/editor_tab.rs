// Represents a single editor tab with its content
use gpui::*;
use gpui_component::highlighter::Language;
use gpui_component::input::{InputState, TabSize};

use crate::lightspeed::settings::EditorSettings;

use super::languages::{language_from_extension, language_name};

#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub file_path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub original_content: String,
    pub encoding: String,
    pub language: Language,
}

// Create a new input state with syntax highlighting
// @param window: The window to create the input state in
// @param cx: The application context
// @param language: The language of the input state
// @param content: The content of the input state
// @param settings: The settings for the input state
// @return: The new input state
fn make_input_state(
    window: &mut Window,
    cx: &mut Context<InputState>,
    language: Language,
    content: Option<String>,
    settings: &EditorSettings,
) -> InputState {
    InputState::new(window, cx)
        .code_editor(language_name(&language).to_string())
        .line_number(settings.show_line_numbers)
        .indent_guides(settings.show_indent_guides)
        .tab_size(TabSize {
            tab_size: settings.tab_size,
            hard_tabs: false,
        })
        .soft_wrap(settings.soft_wrap)
        .default_value(content.unwrap_or_default())
}

impl EditorTab {
    // Create a new tab
    // @param id: The ID of the tab
    // @param title: The title of the tab
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @param settings: The settings for the input state
    // @return: The new tab
    pub fn new(
        id: usize,
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let language = Language::Plain;
        let content = cx.new(|cx| make_input_state(window, cx, language, None, settings));

        Self {
            id,
            title: title.into(),
            content,
            file_path: None,
            modified: false,
            original_content: String::new(),
            encoding: "UTF-8".to_string(),
            language,
        }
    }

    // Create a new tab from a file
    // @param id: The ID of the tab
    // @param path: The path of the file
    // @param contents: The contents of the file
    // @param encoding: The encoding of the file
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @param settings: The settings for the input state
    // @return: The new tab
    pub fn from_file(
        id: usize,
        path: std::path::PathBuf,
        contents: String,
        encoding: String,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
        is_modified: bool,
    ) -> Self {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string();

        // Detect language from file extension
        let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
        let language = language_from_extension(extension);

        let content =
            cx.new(|cx| make_input_state(window, cx, language, Some(contents.clone()), settings));
        let title = format!("{}{}", file_name, if is_modified { " •" } else { "" });
        Self {
            id,
            title: title.into(),
            content,
            file_path: Some(path),
            modified: is_modified,
            original_content: contents,
            encoding,
            language,
        }
    }

    // Update the editor's display settings
    // @param window: The window context
    // @param cx: The application context
    // @param settings: The settings for the input state
    pub fn update_settings(&self, window: &mut Window, cx: &mut App, settings: &EditorSettings) {
        self.content.update(cx, |input_state, cx| {
            input_state.set_line_number(settings.show_line_numbers, window, cx);
            input_state.set_indent_guides(settings.show_indent_guides, window, cx);
            input_state.set_soft_wrap(settings.soft_wrap, window, cx);
            // Note: tab_size cannot be changed after InputState creation
            // It must be set during the initial creation of the InputState
        });
    }

    // Check if the tab's content has been modified
    // @param cx: The application context
    // @return: True if the tab's content has been modified, false otherwise
    pub fn check_modified(&mut self, cx: &mut App) -> bool {
        let current_text = self.content.read(cx).text().to_string();
        self.modified = current_text != self.original_content;
        self.modified
    }

    // Mark the tab as saved
    // @param cx: The application context
    pub fn mark_as_saved(&mut self, cx: &mut App) {
        self.original_content = self.content.read(cx).text().to_string();
        self.modified = false;
    }

    // Update the language/syntax highlighting based on the file extension
    // @param window: The window context
    // @param cx: The application context
    // @param settings: The settings for the input state
    pub fn update_language(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) {
        if let Some(ref path) = self.file_path {
            let extension = path.extension().and_then(|ext| ext.to_str()).unwrap_or("");
            let language = language_from_extension(extension);

            // Get the current content to preserve it
            let current_content = self.content.read(cx).text().to_string();

            // Create a new InputState with the new language
            self.content = cx
                .new(|cx| make_input_state(window, cx, language, Some(current_content), settings));
        }
    }
}
