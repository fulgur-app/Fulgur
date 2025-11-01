// Represents a single editor tab with its content
use gpui::*;
use gpui_component::input::{InputState, TabSize};
use gpui_component::highlighter::Language;

use super::languages::{language_from_extension, language_name};

#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub file_path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub original_content: String,
    pub language: Language,
}

// Create a new input state with syntax highlighting
// @param window: The window to create the input state in
// @param cx: The application context
// @param language: The language of the input state
// @param content: The content of the input state
// @return: The new input state
fn make_input_state(window: &mut Window, cx: &mut Context<InputState>, language: Language, content: Option<String>) -> InputState {
    InputState::new(window, cx)
        .code_editor(language_name(&language).to_string())
        .line_number(true)
        .indent_guides(true)
        .tab_size(TabSize {
            tab_size: 4,
            hard_tabs: false,
        })
        .soft_wrap(false)
        .default_value(content.unwrap_or_default())
}

impl EditorTab {
    // Create a new tab
    // @param id: The ID of the tab
    // @param title: The title of the tab
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @return: The new tab
    pub fn new(id: usize, title: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Self {
        let language = Language::Plain;
        let content = cx.new(|cx| {
            make_input_state(window, cx, language, None)
        });
        
        Self {
            id,
            title: title.into(),
            content,
            file_path: None,
            modified: false,
            original_content: String::new(),
            language,
        }
    }

    // Create a new tab from a file
    // @param id: The ID of the tab
    // @param path: The path of the file
    // @param contents: The contents of the file
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @return: The new tab
    pub fn from_file(
        id: usize,
        path: std::path::PathBuf,
        contents: String,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string();

        // Detect language from file extension
        let extension = path
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let language = language_from_extension(extension);
        
        let content = cx.new(|cx| {
            make_input_state(window, cx, language, Some(contents.clone()))
        });

        Self {
            id,
            title: file_name.into(),
            content,
            file_path: Some(path),
            modified: false,
            original_content: contents,
            language,
        }
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
}
