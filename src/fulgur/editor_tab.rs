// Represents a single editor tab with its content
use gpui::*;
use gpui_component::highlighter::Language;
use gpui_component::input::{InputState, Position, TabSize};
use regex::Regex;

use crate::fulgur::components_utils::{UNTITLED, UTF_8};
use crate::fulgur::settings::EditorSettings;

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
    pub show_markdown_toolbar: bool,
    pub show_markdown_preview: bool,
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
            encoding: UTF_8.to_string(),
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
        }
    }

    // Create a new tab from content with a given file name (no path)
    // Used for shared files from sync server
    // @param id: The ID of the tab
    // @param contents: The contents of the file
    // @param file_name: The name of the file (displayed in tab bar)
    // @param window: The window to create the tab in
    // @param cx: The application context
    // @param settings: The settings for the input state
    // @return: The new tab
    pub fn from_content(
        id: usize,
        contents: String,
        file_name: String,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let extension = std::path::Path::new(&file_name)
            .extension()
            .and_then(|ext| ext.to_str())
            .unwrap_or("");
        let language = language_from_extension(extension);
        let content =
            cx.new(|cx| make_input_state(window, cx, language, Some(contents.clone()), settings));
        Self {
            id,
            title: file_name.into(),
            content,
            file_path: None,                 // No path - forces "Save as..." dialog
            modified: true,                  // Mark as modified
            original_content: String::new(), // Empty so check_modified() keeps it as modified
            encoding: UTF_8.to_string(),
            language,
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
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
            .unwrap_or(UNTITLED)
            .to_string();

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
            show_markdown_toolbar: settings.markdown_settings.show_markdown_toolbar,
            show_markdown_preview: settings.markdown_settings.show_markdown_preview,
        }
    }

    // Update the editor's display settings. Tab size cannot be changed after InputState creation.
    // @param window: The window context
    // @param cx: The application context
    // @param settings: The settings for the input state
    pub fn update_settings(&self, window: &mut Window, cx: &mut App, settings: &EditorSettings) {
        self.content.update(cx, |input_state, cx| {
            input_state.set_line_number(settings.show_line_numbers, window, cx);
            input_state.set_indent_guides(settings.show_indent_guides, window, cx);
            input_state.set_soft_wrap(settings.soft_wrap, window, cx);
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

    // Get suggested filename for "Save as..." dialog
    // Extracts filename from tab title by removing modification indicators
    // @return: The suggested filename, or None if UNTITLED
    pub fn get_suggested_filename(&self) -> Option<String> {
        let title_str = self.title.to_string();
        let cleaned = title_str.trim_end_matches(" •").trim();
        if cleaned.is_empty() || cleaned.starts_with(UNTITLED) {
            None
        } else {
            Some(cleaned.to_string())
        }
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
            self.force_language(window, cx, language, settings);
        }
    }

    // Force the language/syntax highlighting based on the file extension
    // @param window: The window context
    // @param cx: The application context
    // @param language: The language to force
    // @param settings: The settings for the input state
    pub fn force_language(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        language: Language,
        settings: &EditorSettings,
    ) {
        let current_content = self.content.read(cx).text().to_string();
        self.language = language;
        self.content =
            cx.new(|cx| make_input_state(window, cx, language, Some(current_content), settings));
    }

    // Jump to a specific line
    // @param cx: The application context
    // @param line: The line to jump to
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

pub struct Jump {
    pub line: u32,
    pub character: Option<u32>,
}

// Extract the line number and character from a destination string
// @param destination: The destination string
// @return: The jump struct
pub fn extract_line_number(destination: SharedString) -> anyhow::Result<Jump> {
    let mut jump = Jump {
        line: 0,
        character: None,
    };
    let re = Regex::new(r"^(\d+|\d+:\d+)$").unwrap();
    re.is_match(destination.as_str())
        .then(|| {
            if destination.contains(":") {
                let parts = destination.split(":").collect::<Vec<&str>>();
                if parts.len() == 2 {
                    jump.line = string_to_u32(parts[0]) - 1;
                    jump.character = Some(string_to_u32(parts[1]));
                }
            } else {
                jump.line = string_to_u32(destination.as_str()) - 1;
            }
        })
        .ok_or(anyhow::anyhow!("Invalid destination"))?;
    Ok(jump)
}

// Convert a string to a u32
// @param string: The string to convert
// @return: The u32 value of the string, or None if the string is not a valid u32
fn string_to_u32(string: &str) -> u32 {
    if let Ok(line) = string.parse::<u32>() {
        line
    } else {
        0
    }
}

#[cfg(test)]
mod tests {
    use super::extract_line_number;
    use gpui::SharedString;

    #[test]
    fn test_extract_line_number_simple() {
        let destination = SharedString::from("23");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 22);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_with_character() {
        let destination = SharedString::from("23:17");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 22);
        assert_eq!(result.character, Some(17));
    }

    #[test]
    fn test_extract_line_number_invalid() {
        let destination = SharedString::from("azerty");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }
}
