use std::borrow::Cow;

use csscolorparser::{Color as CssColor, ParseColorError};
use gpui_kit::component::input::DocumentColorProvider;
use gpui_kit::{App, Task, Window};
use lsp_types::{Color, ColorInformation, Position, Range};
use ropey::{LineType, Rope};

use super::is_large_file;

/// Lines longer than this (typically minified files) are skipped.
const MAX_LINE_BYTES: usize = 4 * 1024;

/// Maximum distance searched for the closing parenthesis of a color function.
const MAX_FUNCTION_ARGS_BYTES: usize = 64;

/// Color function names recognized before an opening parenthesis.
const COLOR_FUNCTIONS: [&str; 11] = [
    "hsl", "hsla", "rgb", "rgba", "hwb", "hwba", "oklab", "oklch", "lab", "lch", "hsv",
];

/// Provides document color highlighting for CSS color codes in the editor.
pub struct ColorHighlightProvider;

impl DocumentColorProvider for ColorHighlightProvider {
    /// Detect color codes in the editor text on the background executor
    ///
    /// ### Arguments
    /// - `text`: The editor text as a Rope
    /// - `_window`: The window context (unused)
    /// - `cx`: The application context, used to reach the background executor
    ///
    /// ### Returns
    /// - `Task<gpui_kit::Result<Vec<ColorInformation>>>`: The detected colors with their positions
    fn document_colors(
        &self,
        text: &Rope,
        _window: &mut Window,
        cx: &mut App,
    ) -> Task<gpui_kit::Result<Vec<ColorInformation>>> {
        if is_large_file(text.len()) {
            return Task::ready(Ok(Vec::new()));
        }
        let text = text.clone();
        cx.background_executor()
            .spawn(async move { Ok(document_colors_in(&text)) })
    }
}

/// A color found on a single line.
#[derive(Debug, Clone, PartialEq)]
struct LineColor<'a> {
    /// 0-based start column, in characters
    column: u32,
    /// The matched color text
    matched: &'a str,
    /// The parsed color
    color: CssColor,
}

/// Collect every color in a document.
///
/// ### Arguments
/// - `text`: The document text
///
/// ### Returns
/// - `Vec<ColorInformation>`: The detected colors, or none for a large-file-mode sized document
fn document_colors_in(text: &Rope) -> Vec<ColorInformation> {
    if is_large_file(text.len()) {
        return Vec::new();
    }
    let mut colors = Vec::new();
    for (line_index, line) in text.lines(LineType::LF).enumerate() {
        if line.len() > MAX_LINE_BYTES {
            continue;
        }
        let Ok(line_number) = u32::try_from(line_index) else {
            break;
        };
        let line = Cow::from(line);
        colors.extend(
            colors_in_line(&line)
                .into_iter()
                .map(|found| to_color_information(line_number, &found)),
        );
    }
    colors
}

/// Convert a line color into an LSP color information entry.
///
/// ### Arguments
/// - `line_number`: The 0-based line the color was found on
/// - `found`: The color found on that line
///
/// ### Returns
/// - `ColorInformation`: The color with its character-based range
fn to_color_information(line_number: u32, found: &LineColor<'_>) -> ColorInformation {
    let length = u32::try_from(found.matched.chars().count()).unwrap_or(0);
    ColorInformation {
        range: Range {
            start: Position::new(line_number, found.column),
            end: Position::new(line_number, found.column.saturating_add(length)),
        },
        color: Color {
            red: found.color.r,
            green: found.color.g,
            blue: found.color.b,
            alpha: found.color.a,
        },
    }
}

/// Find every color on a single line in one linear pass.
///
/// ### Description
/// All recognized tokens are ASCII, so the scan walks bytes and derives the
/// character column by counting UTF-8 lead bytes. Matches must not be glued to
/// a surrounding identifier, which rejects `{#each}`, `page#add` or `xrgb(`.
///
/// ### Arguments
/// - `line`: The line text, with or without its line terminator
///
/// ### Returns
/// - `Vec<LineColor<'_>>`: The colors found, in order of appearance
fn colors_in_line(line: &str) -> Vec<LineColor<'_>> {
    let bytes = line.as_bytes();
    let mut colors = Vec::new();
    let mut byte = 0;
    let mut column: u32 = 0;
    while byte < bytes.len() {
        let preceded_by_word = byte > 0 && is_word_byte(bytes[byte - 1]);
        let found = match bytes[byte] {
            b'#' if !preceded_by_word => hex_color_at(line, byte),
            b'a'..=b'z' if !preceded_by_word => function_color_at(line, byte),
            _ => None,
        };
        if let Some((end, color)) = found {
            let matched = &line[byte..end];
            colors.push(LineColor {
                column,
                matched,
                color,
            });
            column = column.saturating_add(u32::try_from(matched.chars().count()).unwrap_or(0));
            byte = end;
            continue;
        }
        if is_char_start(bytes[byte]) {
            column = column.saturating_add(1);
        }
        byte += 1;
    }
    colors
}

/// Try to read a hex color starting at a `#`.
///
/// ### Arguments
/// - `line`: The line text
/// - `start`: Byte index of the `#`
///
/// ### Returns
/// - `Some((usize, CssColor))`: The exclusive end byte index and the parsed color
/// - `None`: If the digits are not a valid hex color length or run into a word character
fn hex_color_at(line: &str, start: usize) -> Option<(usize, CssColor)> {
    let bytes = line.as_bytes();
    let digits = bytes[start + 1..]
        .iter()
        .take(9)
        .take_while(|b| b.is_ascii_hexdigit())
        .count();
    if !matches!(digits, 3 | 4 | 6 | 8) {
        return None;
    }
    let end = start + 1 + digits;
    if bytes.get(end).is_some_and(|&b| is_word_byte(b)) {
        return None;
    }
    csscolorparser::parse(&line[start..end])
        .ok()
        .map(|color| (end, color))
}

/// Try to read a color function such as `rgb(...)` starting at its name.
///
/// ### Arguments
/// - `line`: The line text
/// - `start`: Byte index of the first letter of the function name
///
/// ### Returns
/// - `Some((usize, CssColor))`: The exclusive end byte index (after `)`) and the parsed color
/// - `None`: If no known function, closing parenthesis or valid color is found
fn function_color_at(line: &str, start: usize) -> Option<(usize, CssColor)> {
    let bytes = line.as_bytes();
    let name_len = bytes[start..]
        .iter()
        .take(6)
        .take_while(|b| b.is_ascii_lowercase())
        .count();
    let open = start + name_len;
    if bytes.get(open) != Some(&b'(') || !COLOR_FUNCTIONS.contains(&&line[start..open]) {
        return None;
    }
    let close = bytes[open + 1..]
        .iter()
        .take(MAX_FUNCTION_ARGS_BYTES)
        .position(|&b| b == b')')?;
    let end = open + 1 + close + 1;
    parse_color(&line[start..end])
        .ok()
        .map(|color| (end, color))
}

/// Parse a color function, preferring the gpui 0..1 float convention.
///
/// ### Arguments
/// - `text`: The color function text, for example `hsla(0.5, 1.0, 0.5, 1.0)`
///
/// ### Returns
/// - `Ok(CssColor)`: The parsed color
/// - `Err(ParseColorError)`: If the text is not a valid color
fn parse_color(text: &str) -> Result<CssColor, ParseColorError> {
    parse_gpui_color(text).map_or_else(|| csscolorparser::parse(text), Ok)
}

/// Parse `rgb[a]`/`hsl[a]` functions whose arguments are all floats in 0..=1.
///
/// ### Arguments
/// - `text`: The color function text
///
/// ### Returns
/// - `Some(CssColor)`: The parsed color
/// - `None`: If the function or any argument does not follow the gpui convention
fn parse_gpui_color(text: &str) -> Option<CssColor> {
    fn unit_value(value: &str) -> Option<f32> {
        value
            .parse::<f32>()
            .ok()
            .filter(|v| (0.0..=1.0).contains(v))
    }

    let (name, args) = text.strip_suffix(')')?.split_once('(')?;
    let values = args
        .split(',')
        .flat_map(str::split_ascii_whitespace)
        .map(unit_value)
        .collect::<Option<Vec<f32>>>()?;
    let (first, second, third, alpha) = match values.as_slice() {
        [first, second, third] => (*first, *second, *third, 1.0),
        [first, second, third, alpha] => (*first, *second, *third, *alpha),
        _ => return None,
    };
    match name {
        "rgb" | "rgba" => Some(CssColor::new(first, second, third, alpha)),
        "hsl" | "hsla" => Some(CssColor::from_hsla(first * 360.0, second, third, alpha)),
        _ => None,
    }
}

/// Check whether a byte belongs to an ASCII identifier.
///
/// ### Arguments
/// - `byte`: The byte to check
///
/// ### Returns
/// - `bool`: True for ASCII letters, digits and `_`
fn is_word_byte(byte: u8) -> bool {
    byte.is_ascii_alphanumeric() || byte == b'_'
}

/// Check whether a byte starts a UTF-8 character (is not a continuation byte).
///
/// ### Arguments
/// - `byte`: The byte to check
///
/// ### Returns
/// - `bool`: True if the byte starts a new character
fn is_char_start(byte: u8) -> bool {
    byte & 0xC0 != 0x80
}

/// Reports that the document contains no colors.
pub struct NoColorProvider;

impl DocumentColorProvider for NoColorProvider {
    fn document_colors(
        &self,
        _text: &Rope,
        _window: &mut Window,
        _cx: &mut App,
    ) -> Task<gpui_kit::Result<Vec<ColorInformation>>> {
        Task::ready(Ok(Vec::new()))
    }
}

#[cfg(test)]
mod tests {
    use super::{MAX_LINE_BYTES, colors_in_line, document_colors_in};
    use crate::fulgur::ui::tabs::editor_tab::LARGE_FILE_THRESHOLD_BYTES;
    use lsp_types::Position;
    use ropey::Rope;

    /// Helper: return the matched strings found on a single line
    fn extract_colors(text: &str) -> Vec<&str> {
        colors_in_line(text)
            .into_iter()
            .map(|found| found.matched)
            .collect()
    }

    /// Helper: return the (start, end) positions of every color in a document
    fn color_ranges(text: &str) -> Vec<(Position, Position)> {
        document_colors_in(&Rope::from_str(text))
            .into_iter()
            .map(|info| (info.range.start, info.range.end))
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
    fn test_accepts_hex_4_and_8_digit() {
        assert_eq!(
            extract_colors("a: #0f0E; b: #ff003c99;"),
            vec!["#0f0E", "#ff003c99"]
        );
    }

    #[test]
    fn test_rejects_invalid_hex_lengths() {
        assert!(extract_colors("#12 #12345 #1234567 #123456789").is_empty());
    }

    #[test]
    fn test_rejects_hex_glued_to_identifier() {
        assert!(extract_colors("page#add").is_empty());
        assert!(extract_colors("#abc_def").is_empty());
    }

    #[test]
    fn test_accepts_oklch() {
        assert_eq!(
            extract_colors("--color: oklch(70% 0.2 220);"),
            vec!["oklch(70% 0.2 220)"]
        );
    }

    #[test]
    fn test_accepts_rgb() {
        assert_eq!(
            extract_colors("color: rgb(255, 100, 0);"),
            vec!["rgb(255, 100, 0)"]
        );
    }

    #[test]
    fn test_accepts_hsl() {
        assert_eq!(
            extract_colors("color: hsl(225, 100%, 70%);"),
            vec!["hsl(225, 100%, 70%)"]
        );
    }

    #[test]
    fn test_rejects_function_glued_to_identifier() {
        assert!(extract_colors("xrgb(255, 0, 0)").is_empty());
    }

    #[test]
    fn test_rejects_unclosed_function() {
        assert!(extract_colors("rgb(255, 0, 0").is_empty());
    }

    #[test]
    fn test_gpui_float_colors_use_turn_based_hue() {
        let found = colors_in_line("hsla(0.5, 1.0, 0.5, 1.0)");
        assert_eq!(found.len(), 1);
        assert_eq!(found[0].color.to_css_hex(), "#00ffff");
    }

    #[test]
    fn test_multiple_colors_on_same_line() {
        assert_eq!(
            extract_colors("border: #F00 #00FF00;"),
            vec!["#F00", "#00FF00"]
        );
    }

    #[test]
    fn test_hex_at_end_of_line() {
        assert_eq!(extract_colors("#FFF"), vec!["#FFF"]);
    }

    #[test]
    fn test_function_color_at_line_start_has_column_zero() {
        assert_eq!(
            color_ranges("oklch(0.71 0.1435 254.6)"),
            vec![(Position::new(0, 0), Position::new(0, 24))]
        );
    }

    #[test]
    fn test_columns_are_counted_in_characters() {
        assert_eq!(
            color_ranges("/* café */ color: #abc;"),
            vec![(Position::new(0, 18), Position::new(0, 22))]
        );
    }

    #[test]
    fn test_line_numbers_with_crlf() {
        assert_eq!(
            color_ranges("a: #fff;\r\nb: rgb(0, 0, 0);\r\n"),
            vec![
                (Position::new(0, 3), Position::new(0, 7)),
                (Position::new(1, 3), Position::new(1, 15)),
            ]
        );
    }

    #[test]
    fn test_skips_overlong_lines() {
        let long_line = format!("#fff {}", "x".repeat(MAX_LINE_BYTES));
        let text = format!("{long_line}\n#000");
        assert_eq!(
            color_ranges(&text),
            vec![(Position::new(1, 0), Position::new(1, 4))]
        );
    }

    #[test]
    fn test_skips_oversized_documents() {
        let text = format!(
            "#fff\n{}",
            "x\n".repeat(usize::try_from(LARGE_FILE_THRESHOLD_BYTES / 2).unwrap())
        );
        assert!(color_ranges(&text).is_empty());
    }
}
