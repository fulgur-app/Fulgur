mod constructors;
pub mod hex_color_provider;
mod navigation;
mod operations;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

pub use navigation::{Jump, extract_line_number};

use gpui::*;
use gpui_component::input::{InputState, TabSize};
use std::rc::Rc;
use std::time::SystemTime;

use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::settings::EditorSettings;

/// A single editor tab with its content and file metadata
#[derive(Clone)]
pub struct EditorTab {
    pub id: usize,
    pub title: SharedString,
    pub content: Entity<InputState>,
    pub file_path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub original_content: String,
    pub encoding: String,
    pub language: SupportedLanguage,
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
    pub file_size_bytes: Option<u64>,
    pub file_last_modified: Option<SystemTime>,
}

/// All state required to transfer an editor tab between windows
pub struct TabTransferData {
    pub title: SharedString,
    pub content: String,
    pub file_path: Option<std::path::PathBuf>,
    pub modified: bool,
    pub original_content: String,
    pub encoding: String,
    pub language: SupportedLanguage,
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
    pub file_size_bytes: Option<u64>,
    pub file_last_modified: Option<SystemTime>,
    pub cursor_position: gpui_component::input::Position,
}

/// Parameters for creating an editor tab as a duplicate of another
pub struct FromDuplicateParams {
    pub id: usize,
    pub title: SharedString,
    pub current_content: String,
    pub encoding: String,
    pub language: SupportedLanguage,
}

/// Parameters for creating an editor tab from a file
pub struct FromFileParams {
    pub id: usize,
    pub path: std::path::PathBuf,
    pub contents: String,
    pub encoding: String,
    pub is_modified: bool,
}

/// Create a new input state with syntax highlighting
///
/// ### Arguments
/// - `window`: The window to create the input state in
/// - `cx`: The application context
/// - `language_name`: The language registry name for syntax highlighting
/// - `content`: The content of the input state
/// - `settings`: The settings for the input state
///
/// ### Returns
/// - `InputState`: The new input state
fn make_input_state(
    window: &mut Window,
    cx: &mut Context<InputState>,
    language_name: &str,
    content: Option<String>,
    settings: &EditorSettings,
) -> InputState {
    let mut state = InputState::new(window, cx)
        .code_editor(language_name.to_string())
        .line_number(settings.show_line_numbers)
        .indent_guides(settings.show_indent_guides)
        .tab_size(TabSize {
            tab_size: settings.tab_size,
            hard_tabs: !settings.use_spaces,
        })
        .soft_wrap(settings.soft_wrap)
        .show_whitespaces(settings.show_whitespaces)
        .default_value(content.unwrap_or_default());

    if settings.highlight_colors {
        state.lsp.document_color_provider =
            Some(Rc::new(hex_color_provider::ColorHighlightProvider));
    }

    state
}
