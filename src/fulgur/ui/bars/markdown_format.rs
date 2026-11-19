use std::ops::Range;

const CODE_FENCE: &str = "```";
const THEMATIC_BREAK: &str = "---";

/// A formatting action offered by the markdown toolbar
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum MarkdownFormat {
    /// Surround the selection with `prefix` and `suffix`, or insert both at the cursor
    Surround {
        prefix: &'static str,
        suffix: &'static str,
    },
    /// Insert the prefix at the start of every line the selection touches
    LinePrefix(&'static str),
    /// Wrap the selection in a fenced code block whose fences sit on their own lines
    CodeBlock,
    /// Insert a thematic break on its own line, after a blank line
    Separator,
}

/// A single replacement planned against a buffer
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct MarkdownEdit {
    /// The UTF-8 byte range of the original text to replace
    pub(crate) range: Range<usize>,
    /// The text written over `range`
    pub(crate) replacement: String,
    /// The UTF-8 byte range to select in the edited text
    pub(crate) selection: Range<usize>,
}

/// Plan the edit a markdown toolbar action makes to a buffer
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `selection`: The selected UTF-8 byte range, empty for a bare cursor
/// - `format`: The formatting action to apply
///
/// ### Returns
/// - `MarkdownEdit`: The replacement to apply and the selection to restore afterwards
pub(crate) fn plan_markdown_edit(
    text: &str,
    selection: Range<usize>,
    format: MarkdownFormat,
) -> MarkdownEdit {
    match format {
        MarkdownFormat::Surround { prefix, suffix } => {
            plan_surround(text, selection, prefix, suffix)
        }
        MarkdownFormat::LinePrefix(prefix) => plan_line_prefix(text, selection, prefix),
        MarkdownFormat::CodeBlock => plan_code_block(text, selection),
        MarkdownFormat::Separator => plan_separator(text, selection),
    }
}

/// Plan wrapping the selection with a prefix and suffix, leaving the cursor after the suffix
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `selection`: The selected UTF-8 byte range
/// - `prefix`: The text written before the selection
/// - `suffix`: The text written after the selection
///
/// ### Returns
/// - `MarkdownEdit`: The planned replacement
fn plan_surround(text: &str, selection: Range<usize>, prefix: &str, suffix: &str) -> MarkdownEdit {
    let replacement = format!("{prefix}{}{suffix}", &text[selection.clone()]);
    let caret = selection.start + replacement.len();
    MarkdownEdit {
        range: selection,
        replacement,
        selection: caret..caret,
    }
}

/// Plan prefixing every line the selection touches, keeping the selection on the same text
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `selection`: The selected UTF-8 byte range
/// - `prefix`: The marker written at the start of each line
///
/// ### Returns
/// - `MarkdownEdit`: The planned replacement
fn plan_line_prefix(text: &str, selection: Range<usize>, prefix: &str) -> MarkdownEdit {
    let block_start = line_start(text, selection.start);
    let block_end = line_bounds(text, last_selected_offset(text, &selection)).end;
    let block = &text[block_start..block_end];

    let mut replacement = String::with_capacity(block.len() + prefix.len());
    let mut line_count = 0;
    for line in block.split_inclusive('\n') {
        replacement.push_str(prefix);
        replacement.push_str(line);
        line_count += 1;
    }
    if block.is_empty() {
        replacement.push_str(prefix);
        line_count = 1;
    }

    MarkdownEdit {
        range: block_start..block_end,
        replacement,
        selection: selection.start + prefix.len()..selection.end + prefix.len() * line_count,
    }
}

/// Plan wrapping the selection in a fenced code block, opening a line before the fence and
/// after the closing fence when the selection does not already sit on line boundaries
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `selection`: The selected UTF-8 byte range
///
/// ### Returns
/// - `MarkdownEdit`: The planned replacement, selecting the wrapped code, or placing the
///   cursor on the empty line inside the block when nothing was selected
fn plan_code_block(text: &str, selection: Range<usize>) -> MarkdownEdit {
    let newline = newline_of(text);
    let selected = &text[selection.clone()];
    let starts_line = line_start(text, selection.start) == selection.start;
    let ends_line = line_bounds(text, selection.end).end == selection.end;

    let mut replacement = String::new();
    if !starts_line {
        replacement.push_str(newline);
    }
    replacement.push_str(CODE_FENCE);
    replacement.push_str(newline);
    let content_start = selection.start + replacement.len();
    replacement.push_str(selected);
    if !selected.ends_with('\n') {
        replacement.push_str(newline);
    }
    replacement.push_str(CODE_FENCE);
    if !ends_line {
        replacement.push_str(newline);
    }

    MarkdownEdit {
        range: selection,
        replacement,
        selection: content_start..content_start + selected.len(),
    }
}

/// Plan inserting a thematic break after the last line the selection touches
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `selection`: The selected UTF-8 byte range
///
/// ### Returns
/// - `MarkdownEdit`: The planned replacement, leaving the cursor on the line after the break
fn plan_separator(text: &str, selection: Range<usize>) -> MarkdownEdit {
    let newline = newline_of(text);
    let line = line_bounds(text, last_selected_offset(text, &selection));

    let (range, leading_newlines) = if text[line.clone()].trim().is_empty() {
        let follows_text = line.start > 0
            && !text[line_start(text, line.start - 1)..line.start]
                .trim()
                .is_empty();
        (line, usize::from(follows_text))
    } else {
        (line.end..line.end, 2)
    };

    let mut replacement = newline.repeat(leading_newlines);
    replacement.push_str(THEMATIC_BREAK);
    replacement.push_str(newline);

    let caret = range.start + replacement.len();
    MarkdownEdit {
        range,
        replacement,
        selection: caret..caret,
    }
}

/// Byte offset of the start of the line containing `offset`
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `offset`: A UTF-8 byte offset into `text`
///
/// ### Returns
/// - `usize`: The offset just after the previous line feed, or 0 on the first line
fn line_start(text: &str, offset: usize) -> usize {
    text[..offset].rfind('\n').map_or(0, |index| index + 1)
}

/// Byte range of the line containing `offset`, without its line terminator
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `offset`: A UTF-8 byte offset into `text`
///
/// ### Returns
/// - `Range<usize>`: The line's content, excluding a trailing `\n` or `\r\n`
fn line_bounds(text: &str, offset: usize) -> Range<usize> {
    let start = line_start(text, offset);
    let end = text[start..]
        .find('\n')
        .map_or(text.len(), |index| start + index);
    let end = if text[start..end].ends_with('\r') {
        end - 1
    } else {
        end
    };
    start..end
}

/// Byte offset inside the last line a selection touches
///
/// ### Arguments
/// - `text`: The whole buffer
/// - `selection`: The selected UTF-8 byte range
///
/// ### Returns
/// - `usize`: An offset on the last line the selection covers
fn last_selected_offset(text: &str, selection: &Range<usize>) -> usize {
    if selection.end > selection.start && line_start(text, selection.end) == selection.end {
        selection.end - 1
    } else {
        selection.end
    }
}

/// The line terminator inserted text should use to match the buffer
///
/// ### Arguments
/// - `text`: The whole buffer
///
/// ### Returns
/// - `&'static str`: `"\r\n"` when the buffer already uses CRLF, `"\n"` otherwise
fn newline_of(text: &str) -> &'static str {
    if text.contains("\r\n") { "\r\n" } else { "\n" }
}

#[cfg(test)]
mod tests {
    use super::{MarkdownFormat, plan_markdown_edit};
    use std::ops::Range;

    /// Plan an edit and apply it to a copy of `text`
    ///
    /// ### Arguments
    /// - `text`: The buffer before the edit
    /// - `selection`: The selected UTF-8 byte range
    /// - `format`: The formatting action to apply
    ///
    /// ### Returns
    /// - `(String, String)`: The edited buffer, and the text selected afterwards
    fn apply(text: &str, selection: Range<usize>, format: MarkdownFormat) -> (String, String) {
        let edit = plan_markdown_edit(text, selection, format);
        let mut edited = text.to_string();
        edited.replace_range(edit.range, &edit.replacement);
        let selected = edited[edit.selection].to_string();
        (edited, selected)
    }

    /// Plan an edit and return where the cursor lands
    ///
    /// ### Arguments
    /// - `text`: The buffer before the edit
    /// - `selection`: The selected UTF-8 byte range
    /// - `format`: The formatting action to apply
    ///
    /// ### Returns
    /// - `(String, usize)`: The edited buffer, and the collapsed cursor offset
    fn apply_with_cursor(
        text: &str,
        selection: Range<usize>,
        format: MarkdownFormat,
    ) -> (String, usize) {
        let edit = plan_markdown_edit(text, selection, format);
        assert!(edit.selection.is_empty(), "expected a collapsed cursor");
        let mut edited = text.to_string();
        edited.replace_range(edit.range, &edit.replacement);
        (edited, edit.selection.start)
    }

    const HEADING_1: MarkdownFormat = MarkdownFormat::LinePrefix("# ");
    const LIST: MarkdownFormat = MarkdownFormat::LinePrefix("- ");
    const QUOTE: MarkdownFormat = MarkdownFormat::LinePrefix("> ");

    #[test]
    fn test_surround_wraps_selection_and_moves_cursor_after_suffix() {
        let format = MarkdownFormat::Surround {
            prefix: "**",
            suffix: "**",
        };
        assert_eq!(
            apply_with_cursor("say hello", 4..9, format),
            ("say **hello**".to_string(), 13)
        );
    }

    #[test]
    fn test_line_prefix_with_cursor_mid_line_prefixes_line_start() {
        let edit = plan_markdown_edit("Hello world", 6..6, HEADING_1);
        assert_eq!(edit.range, 0..11);
        assert_eq!(edit.replacement, "# Hello world");
        assert_eq!(edit.selection, 8..8);
    }

    #[test]
    fn test_line_prefix_only_touches_the_cursor_line() {
        let (text, _) = apply("intro\nHello world\noutro", 12..12, HEADING_1);
        assert_eq!(text, "intro\n# Hello world\noutro");
    }

    #[test]
    fn test_line_prefix_on_empty_buffer_inserts_marker() {
        assert_eq!(apply_with_cursor("", 0..0, LIST), ("- ".to_string(), 2));
    }

    #[test]
    fn test_line_prefix_prefixes_every_selected_line() {
        assert_eq!(
            apply("a\nb\nc", 0..5, LIST),
            ("- a\n- b\n- c".to_string(), "a\n- b\n- c".to_string())
        );
    }

    #[test]
    fn test_line_prefix_covers_partially_selected_lines() {
        assert_eq!(
            apply("one\ntwo\nthree", 2..5, LIST),
            ("- one\n- two\nthree".to_string(), "e\n- t".to_string())
        );
    }

    #[test]
    fn test_line_prefix_skips_line_where_selection_ends_at_its_start() {
        let (text, _) = apply("a\nb\n", 0..2, LIST);
        assert_eq!(text, "- a\nb\n");
    }

    #[test]
    fn test_line_prefix_keeps_crlf_line_endings() {
        let (text, _) = apply("a\r\nb", 0..4, QUOTE);
        assert_eq!(text, "> a\r\n> b");
    }

    #[test]
    fn test_code_block_at_mid_line_cursor_puts_fences_on_own_lines() {
        assert_eq!(
            apply_with_cursor("abcdef", 3..3, MarkdownFormat::CodeBlock),
            ("abc\n```\n\n```\ndef".to_string(), 8)
        );
    }

    #[test]
    fn test_code_block_wraps_whole_line_selection() {
        assert_eq!(
            apply("x\nlet a = 1;\ny", 2..12, MarkdownFormat::CodeBlock),
            (
                "x\n```\nlet a = 1;\n```\ny".to_string(),
                "let a = 1;".to_string()
            )
        );
    }

    #[test]
    fn test_code_block_reuses_selection_trailing_newline() {
        assert_eq!(
            apply("let a;\n", 0..7, MarkdownFormat::CodeBlock),
            ("```\nlet a;\n```".to_string(), "let a;\n".to_string())
        );
    }

    #[test]
    fn test_code_block_matches_crlf_line_endings() {
        let (text, _) = apply("a\r\nb", 0..1, MarkdownFormat::CodeBlock);
        assert_eq!(text, "```\r\na\r\n```\r\nb");
    }

    #[test]
    fn test_separator_on_text_line_goes_after_it_behind_a_blank_line() {
        assert_eq!(
            apply_with_cursor("para", 2..2, MarkdownFormat::Separator),
            ("para\n\n---\n".to_string(), 10)
        );
    }

    #[test]
    fn test_separator_on_text_line_followed_by_text() {
        let (text, _) = apply_with_cursor("para\nnext", 0..0, MarkdownFormat::Separator);
        assert_eq!(text, "para\n\n---\n\nnext");
    }

    #[test]
    fn test_separator_on_empty_line_under_paragraph_adds_blank_line() {
        assert_eq!(
            apply_with_cursor("para\n", 5..5, MarkdownFormat::Separator),
            ("para\n\n---\n".to_string(), 10)
        );
    }

    #[test]
    fn test_separator_on_blank_line_after_blank_line_takes_its_place() {
        let (text, _) = apply_with_cursor("para\n\n\nnext", 6..6, MarkdownFormat::Separator);
        assert_eq!(text, "para\n\n---\n\nnext");
    }

    #[test]
    fn test_separator_in_empty_buffer() {
        assert_eq!(
            apply_with_cursor("", 0..0, MarkdownFormat::Separator),
            ("---\n".to_string(), 4)
        );
    }

    #[test]
    fn test_separator_goes_after_last_selected_line() {
        let (text, _) = apply_with_cursor("one\ntwo\nthree", 1..5, MarkdownFormat::Separator);
        assert_eq!(text, "one\ntwo\n\n---\n\nthree");
    }
}
