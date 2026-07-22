use gpui::{App, Task, Window};
use gpui_component::input::DocumentColorProvider;
use lsp_types::{Color, ColorInformation, Position, Range};
use ropey::Rope;

/// Provides document color highlighting for CSS color codes in the editor.
///
/// Post-filters results to reject false positives like `#each` in Svelte where
/// the hex match `#eac` is followed by trailing alphabetic characters.
pub struct ColorHighlightProvider;

impl DocumentColorProvider for ColorHighlightProvider {
    /// Parse color codes from editor text and return color information
    ///
    /// ### Arguments
    /// - `text`: The editor text as a Rope
    /// - `_window`: The window context (unused)
    /// - `_cx`: The application context (unused)
    ///
    /// ### Returns
    /// - `Task<gpui::Result<Vec<ColorInformation>>>`: The detected colors with their positions
    fn document_colors(
        &self,
        text: &Rope,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<gpui::Result<Vec<ColorInformation>>> {
        let text_str = text.to_string();
        let lines: Vec<&str> = text_str.lines().collect();
        let nodes = color_lsp::parse(&text_str);
        let colors = nodes
            .into_iter()
            .filter(|node| {
                let line_idx = node.position.line as usize;
                let match_end = node.position.character as usize + node.matched.chars().count();
                if let Some(line) = lines.get(line_idx) {
                    let line_bytes = line.as_bytes();
                    // Reject hex matches followed by an alphabetic char (e.g. #each in Svelte)
                    if node.matched.starts_with('#')
                        && match_end < line_bytes.len()
                        && line_bytes[match_end].is_ascii_alphabetic()
                    {
                        return false;
                    }
                }
                true
            })
            .map(|node| {
                let line = lines.get(node.position.line as usize).copied();
                let character = corrected_character(line, &node.matched, node.position.character);
                let start = Position::new(node.position.line, character);
                let end = Position::new(
                    node.position.line,
                    character + u32::try_from(node.matched.chars().count()).unwrap_or(0),
                );
                let lsp_color = node.lsp_color();
                ColorInformation {
                    range: Range { start, end },
                    color: Color {
                        red: lsp_color.red,
                        green: lsp_color.green,
                        blue: lsp_color.blue,
                        alpha: lsp_color.alpha,
                    },
                }
            })
            .collect();
        Task::ready(Ok(colors))
    }
}

/// Correct the reported start column of a color match.
///
/// ### Description
/// `color-lsp` 0.2.0 has an off-by-one in its function-color branch: a color
/// starting at column 0 (for example `oklch(...)` or `hsla(...)` at the very start
/// of a line) is reported one column too far right, leaving its first character
/// unhighlighted.
///
/// ### Arguments
/// - `line`: The source line the match was found on, if available
/// - `matched`: The matched color text
/// - `character`: The reported 0-based start column (character index)
///
/// ### Returns
/// - `u32`: The corrected 0-based start column
fn corrected_character(line: Option<&str>, matched: &str, character: u32) -> u32 {
    let Some(line) = line else {
        return character;
    };
    let byte_offset = |ch: u32| line.char_indices().nth(ch as usize).map(|(byte, _)| byte);
    if let Some(byte) = byte_offset(character)
        && line[byte..].starts_with(matched)
    {
        return character;
    }
    if character > 0
        && let Some(byte) = byte_offset(character - 1)
        && line[byte..].starts_with(matched)
    {
        return character - 1;
    }
    character
}

#[cfg(test)]
mod tests {
    use super::corrected_character;

    #[test]
    fn test_corrects_function_color_at_line_start() {
        assert_eq!(
            corrected_character(
                Some("oklch(0.71 0.1435 254.6)"),
                "oklch(0.71 0.1435 254.6)",
                1
            ),
            0
        );
        assert_eq!(
            corrected_character(
                Some("hsla(142, 76%, 36%, 1.00)"),
                "hsla(142, 76%, 36%, 1.00)",
                1
            ),
            0
        );
    }

    #[test]
    fn test_leaves_correct_position_untouched() {
        assert_eq!(corrected_character(Some("  #E9570C"), "#E9570C", 2), 2);
        assert_eq!(
            corrected_character(
                Some("color: hsl(225, 100%, 70%);"),
                "hsl(225, 100%, 70%)",
                7
            ),
            7
        );
    }

    #[test]
    fn test_missing_line_returns_input() {
        assert_eq!(corrected_character(None, "#FFF", 3), 3);
    }

    /// Helper: run `color_lsp::parse` with our post-filter and return matched strings
    fn extract_colors(text: &str) -> Vec<String> {
        let lines: Vec<&str> = text.lines().collect();
        color_lsp::parse(text)
            .into_iter()
            .filter(|node| {
                let line_idx = node.position.line as usize;
                let match_end = node.position.character as usize + node.matched.chars().count();
                if let Some(line) = lines.get(line_idx) {
                    let line_bytes = line.as_bytes();
                    if node.matched.starts_with('#')
                        && match_end < line_bytes.len()
                        && line_bytes[match_end].is_ascii_alphabetic()
                    {
                        return false;
                    }
                }
                true
            })
            .map(|n| n.matched)
            .collect()
    }

    #[test]
    fn test_rejects_svelte_each() {
        assert!(extract_colors("{#each items as item}").is_empty());
    }

    #[test]
    fn test_rejects_svelte_if() {
        assert!(extract_colors("{#if condition}").is_empty());
    }

    #[test]
    fn test_accepts_hex_3_digit() {
        assert_eq!(extract_colors("color: #F0A;"), vec!["#F0A"]);
    }

    #[test]
    fn test_accepts_hex_6_digit() {
        assert_eq!(extract_colors("color: #FF00AA;"), vec!["#FF00AA"]);
    }

    #[test]
    fn test_accepts_oklch() {
        let result = extract_colors("--color: oklch(70% 0.2 220);");
        assert_eq!(result.len(), 1);
        assert!(result[0].starts_with("oklch("));
    }

    #[test]
    fn test_accepts_rgb() {
        let result = extract_colors("color: rgb(255, 100, 0);");
        assert_eq!(result.len(), 1);
        assert!(result[0].starts_with("rgb("));
    }

    #[test]
    fn test_accepts_hsl() {
        let result = extract_colors("color: hsl(225, 100%, 70%);");
        assert_eq!(result.len(), 1);
        assert!(result[0].starts_with("hsl("));
    }

    #[test]
    fn test_multiple_colors_on_same_line() {
        let result = extract_colors("border: #F00 #00FF00;");
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_hex_at_end_of_line() {
        assert_eq!(extract_colors("#FFF"), vec!["#FFF"]);
    }
}
