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
                let start = Position::new(node.position.line, node.position.character);
                let end = Position::new(
                    node.position.line,
                    node.position.character + node.matched.chars().count() as u32,
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

#[cfg(test)]
mod tests {
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
