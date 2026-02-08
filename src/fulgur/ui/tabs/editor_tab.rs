// Represents a single editor tab with its content
use gpui::*;
use gpui_component::highlighter::Language;
use gpui_component::input::{InputState, Position, TabSize};
use regex::Regex;

use crate::fulgur::settings::EditorSettings;
use crate::fulgur::ui::components_utils::{UNTITLED, UTF_8};
use crate::fulgur::ui::languages::{SupportedLanguage, language_from_extension, to_language};

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
}

/// Create a new input state with syntax highlighting
///
/// ### Arguments
/// - `window`: The window to create the input state in
/// - `cx`: The application context
/// - `language`: The language of the input state
/// - `content`: The content of the input state
/// - `settings`: The settings for the input state
///
/// ### Returns
/// - `InputState`: The new input state
fn make_input_state(
    window: &mut Window,
    cx: &mut Context<InputState>,
    language: Language,
    content: Option<String>,
    settings: &EditorSettings,
) -> InputState {
    InputState::new(window, cx)
        .code_editor(language.name().to_string())
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
    /// Create a new tab
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `title`: The title of the tab
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
    pub fn new(
        id: usize,
        title: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut App,
        settings: &EditorSettings,
    ) -> Self {
        let language = SupportedLanguage::Plain;
        let content =
            cx.new(|cx| make_input_state(window, cx, to_language(&language), None, settings));
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

    /// Create a new tab from content with a given file name (no path)
    /// Used for shared files from sync server
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `contents`: The contents of the file
    /// - `file_name`: The name of the file (displayed in tab bar)
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
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
        let content = cx.new(|cx| {
            make_input_state(
                window,
                cx,
                to_language(&language),
                Some(contents.clone()),
                settings,
            )
        });
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

    /// Create a new tab from a file
    ///
    /// ### Arguments
    /// - `id`: The ID of the tab
    /// - `path`: The path of the file
    /// - `contents`: The contents of the file
    /// - `encoding`: The encoding of the file
    /// - `window`: The window to create the tab in
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    /// - `is_modified`: Whether the file is modified
    ///
    /// ### Returns
    /// - `EditorTab`: The new tab
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
        let content = cx.new(|cx| {
            make_input_state(
                window,
                cx,
                to_language(&language),
                Some(contents.clone()),
                settings,
            )
        });
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

    /// Update the editor's display settings. Tab size cannot be changed after InputState creation.
    ///
    /// ### Arguments
    /// - `window`: The window context
    ///
    /// - `cx`: The application context
    /// - `settings`: The settings for the input state
    ///
    /// ### Returns
    /// - `InputState`: The updated input state
    pub fn update_settings(&self, window: &mut Window, cx: &mut App, settings: &EditorSettings) {
        self.content.update(cx, |input_state, cx| {
            input_state.set_line_number(settings.show_line_numbers, window, cx);
            input_state.set_indent_guides(settings.show_indent_guides, window, cx);
            input_state.set_soft_wrap(settings.soft_wrap, window, cx);
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
    /// - `settings`: The settings for the input state
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

    /// Force the language/syntax highlighting based on the file extension
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `language`: The language to force
    /// - `settings`: The settings for the input state
    pub fn force_language(
        &mut self,
        window: &mut Window,
        cx: &mut App,
        language: SupportedLanguage,
        settings: &EditorSettings,
    ) {
        let current_content = self.content.read(cx).text().to_string();
        self.language = language;
        self.content = cx.new(|cx| {
            make_input_state(
                window,
                cx,
                to_language(&language),
                Some(current_content),
                settings,
            )
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

pub struct Jump {
    pub line: u32,
    pub character: Option<u32>,
}

/// Extract the line number and character from a destination string
///
/// ### Arguments
/// - `destination`: The destination string
///
/// ### Returns
/// - `Ok(Jump)`: The jump struct
/// - `Err(anyhow::Error)`: If the destination string is not a valid jump
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
                    let line = string_to_u32(parts[0]);
                    jump.line = if line > 0 { line - 1 } else { 0 };
                    jump.character = Some(string_to_u32(parts[1]));
                }
            } else {
                let line = string_to_u32(destination.as_str());
                jump.line = if line > 0 { line - 1 } else { 0 };
            }
        })
        .ok_or(anyhow::anyhow!("Invalid destination"))?;
    Ok(jump)
}

/// Convert a string to a u32
///
/// ### Arguments
/// - `string`: The string to convert
///
/// ### Returns
/// - `u32`: The u32 value of the string, or 0 if the string is not a valid u32
fn string_to_u32(string: &str) -> u32 {
    match string.parse::<u32>() {
        Ok(line) => line,
        Err(e) => match e.kind() {
            std::num::IntErrorKind::PosOverflow => u32::MAX,
            _ => 0,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{extract_line_number, string_to_u32};
    use gpui::SharedString;

    // ========== extract_line_number() tests ==========

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

    #[test]
    fn test_extract_line_number_line_1() {
        // Line 1 should convert to index 0
        let destination = SharedString::from("1");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 0);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_line_0() {
        // Line 0 should lead to the first line
        let destination = SharedString::from("0");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 0);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_negative() {
        // Negative numbers should fail regex validation
        let destination = SharedString::from("-5");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_very_large() {
        // Very large valid number
        let destination = SharedString::from("999999999");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 999999998);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_overflow() {
        // Number larger than u32::MAX should cause parse to fail, returning 0
        let destination = SharedString::from("99999999999999999999");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, u32::MAX - 1);
        assert_eq!(result.character, None);
    }

    #[test]
    fn test_extract_line_number_non_numeric() {
        // Non-numeric input should fail regex validation
        let destination = SharedString::from("abc");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_partial_numeric() {
        // Partially numeric input should fail regex validation
        let destination = SharedString::from("123abc");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_malformed_colon() {
        // Malformed input with double colon should fail regex validation
        let destination = SharedString::from("file.txt::");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_missing_number_after_colon() {
        // "line:" without number should fail regex validation
        let destination = SharedString::from("23:");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_missing_number_before_colon() {
        // ":23" without line number should fail regex validation
        let destination = SharedString::from(":23");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_empty_string() {
        // Empty string should fail regex validation
        let destination = SharedString::from("");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_with_whitespace() {
        // Whitespace should fail regex validation
        let destination = SharedString::from(" 23 ");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_zero_character() {
        // Line with character 0
        let destination = SharedString::from("10:0");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 9);
        assert_eq!(result.character, Some(0));
    }

    #[test]
    fn test_extract_line_number_three_parts() {
        // Three parts separated by colons should fail regex validation
        let destination = SharedString::from("10:5:2");
        let result = extract_line_number(destination);
        assert!(result.is_err());
    }

    #[test]
    fn test_extract_line_number_with_character_overflow() {
        // Very large character position
        let destination = SharedString::from("10:99999999999999999999");
        let result = extract_line_number(destination).unwrap();
        assert_eq!(result.line, 9);
        // Overflow causes parse to fail, string_to_u32 returns 0
        assert_eq!(result.character, Some(u32::MAX));
    }

    // ========== string_to_u32() tests ==========

    #[test]
    fn test_string_to_u32_valid() {
        assert_eq!(string_to_u32("123"), 123);
        assert_eq!(string_to_u32("0"), 0);
        assert_eq!(string_to_u32("1"), 1);
    }

    #[test]
    fn test_string_to_u32_invalid_non_numeric() {
        // Invalid strings return 0
        assert_eq!(string_to_u32("abc"), 0);
        assert_eq!(string_to_u32("xyz123"), 0);
        assert_eq!(string_to_u32("123abc"), 0);
    }

    #[test]
    fn test_string_to_u32_negative() {
        // Negative numbers return 0
        assert_eq!(string_to_u32("-5"), 0);
        assert_eq!(string_to_u32("-123"), 0);
    }

    #[test]
    fn test_string_to_u32_overflow() {
        // Numbers larger than u32::MAX return 0
        assert_eq!(string_to_u32("99999999999999999999"), u32::MAX);
        assert_eq!(string_to_u32("4294967296"), u32::MAX);
    }

    #[test]
    fn test_string_to_u32_max_value() {
        // u32::MAX should parse correctly
        assert_eq!(string_to_u32("4294967295"), u32::MAX);
    }

    #[test]
    fn test_string_to_u32_empty_string() {
        // Empty string returns 0
        assert_eq!(string_to_u32(""), 0);
    }

    #[test]
    fn test_string_to_u32_whitespace() {
        // Whitespace returns 0 (parse fails)
        assert_eq!(string_to_u32(" "), 0);
        assert_eq!(string_to_u32("  123  "), 0);
        assert_eq!(string_to_u32("\t123\n"), 0);
    }

    #[test]
    fn test_string_to_u32_special_characters() {
        // Special characters return 0
        assert_eq!(string_to_u32("!@#$"), 0);
        assert_eq!(string_to_u32("12.34"), 0); // Decimal point
        assert_eq!(string_to_u32("12,345"), 0); // Comma
    }
}
