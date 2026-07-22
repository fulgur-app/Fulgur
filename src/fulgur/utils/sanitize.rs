/// Sanitize a filename to prevent path traversal and other security issues
///
/// This function:
/// - Extracts just the filename from paths (e.g., "/path/to/file.txt" → "file.txt")
/// - Preserves leading dots (hidden files like ".gitignore")
/// - Rejects bare `..` and `.` directory references
/// - Removes control characters and null bytes
///
/// ### Arguments
/// - `filename`: The filename or path to sanitize
///
/// ### Returns
/// - `String`: The sanitized filename, or "untitled" if the result is empty
///
/// ### Examples
/// ```
/// # use fulgur::fulgur::utils::sanitize::sanitize_filename;
/// assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
/// assert_eq!(sanitize_filename("normal.txt"), "normal.txt");
/// assert_eq!(sanitize_filename("path/to/file.txt"), "file.txt");
/// assert_eq!(sanitize_filename(".hidden"), ".hidden");
/// assert_eq!(sanitize_filename(""), "untitled");
/// ```
#[must_use]
pub fn sanitize_filename(filename: &str) -> String {
    // Normalize path separators to Unix style, then split and take the last component
    let normalized = filename.replace('\\', "/");
    let base_name = normalized.split('/').rfind(|s| !s.is_empty()).unwrap_or("");

    // Remove control characters and null bytes
    let sanitized = base_name
        .replace('\0', "")
        .chars()
        .filter(|c| !c.is_control())
        .collect::<String>();

    // Reject special directory references even if they survived path splitting
    if sanitized == ".." || sanitized == "." {
        return "untitled".to_string();
    }

    // If the result is empty or only whitespace, return a default name
    if sanitized.trim().is_empty() {
        "untitled".to_string()
    } else {
        sanitized
    }
}

/// Neutralize inline newlines and `<br>` tags in a Markdown preview source.
///
/// ### Description
/// gpui-component's Markdown renderer shapes each inline run with gpui's
/// single-line shaper, which asserts (a `debug_assert!`) that the run contains no
/// `\n`; in a debug build that assertion aborts the whole app. Two things feed it
/// a newline:
/// - The `markdown` crate stores a soft line break as a literal `\n` inside a
///   `Text` node (for example `Text("line one\nline two")`), and gpui-component
///   forwards that text verbatim. Any wrapped paragraph, badge block, or
///   multi-line raw-HTML block triggers it.
/// - gpui-component turns a raw `<br>` tag into an `InlineNode::new("\n")`, so a
///   single `<br>` aborts too.
///
/// ### Arguments
/// - `source`: The raw Markdown text about to be rendered in the preview
///
/// ### Returns
/// - `String`: The Markdown with inline newlines and raw `<br>` tags replaced by
///   spaces; the source unchanged when it parses to no offending span (or fails
///   to parse)
#[must_use]
pub fn sanitize_markdown_preview(source: &str) -> String {
    let Ok(ast) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return source.to_string();
    };

    let mut text_ranges: Vec<(usize, usize)> = Vec::new();
    let mut html_ranges: Vec<(usize, usize)> = Vec::new();
    collect_inline_ranges(&ast, &mut text_ranges, &mut html_ranges);
    if text_ranges.is_empty() && html_ranges.is_empty() {
        return source.to_string();
    }

    let text_ranges = merge_ranges(text_ranges);
    let html_ranges = merge_ranges(html_ranges);
    rewrite_inline(source, &text_ranges, &html_ranges)
}

/// Collect the byte ranges of inline text-bearing Markdown nodes.
///
/// ### Arguments
/// - `node`: The current AST node being visited
/// - `text_ranges`: Accumulator for every inline text span
/// - `html_ranges`: Accumulator for raw `Html` spans only
fn collect_inline_ranges(
    node: &markdown::mdast::Node,
    text_ranges: &mut Vec<(usize, usize)>,
    html_ranges: &mut Vec<(usize, usize)>,
) {
    use markdown::mdast::Node;

    if let Some(position) = node.position() {
        let range = (position.start.offset, position.end.offset);
        match node {
            Node::Text(_) | Node::InlineCode(_) | Node::InlineMath(_) => text_ranges.push(range),
            Node::Html(_) => {
                text_ranges.push(range);
                html_ranges.push(range);
            }
            _ => {}
        }
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_inline_ranges(child, text_ranges, html_ranges);
        }
    }
}

/// Merge sorted, possibly overlapping byte ranges into disjoint ones.
///
/// ### Arguments
/// - `ranges`: The `(start, end)` ranges to merge; not required to be sorted
///
/// ### Returns
/// - `Vec<(usize, usize)>`: Disjoint ranges sorted by start offset
fn merge_ranges(mut ranges: Vec<(usize, usize)>) -> Vec<(usize, usize)> {
    ranges.sort_unstable_by_key(|&(start, _)| start);
    let mut merged: Vec<(usize, usize)> = Vec::with_capacity(ranges.len());
    for (start, end) in ranges {
        if let Some(last) = merged.last_mut()
            && start <= last.1
        {
            last.1 = last.1.max(end);
        } else {
            merged.push((start, end));
        }
    }
    merged
}

/// Rewrite the source, neutralizing inline newlines and raw `<br>` tags.
///
/// ### Arguments
/// - `source`: The original Markdown text
/// - `text_ranges`: Disjoint, start-sorted byte ranges of inline text spans
/// - `html_ranges`: Disjoint, start-sorted byte ranges of raw `Html` spans
///
/// ### Returns
/// - `String`: The rewritten Markdown
fn rewrite_inline(
    source: &str,
    text_ranges: &[(usize, usize)],
    html_ranges: &[(usize, usize)],
) -> String {
    let mut output = String::with_capacity(source.len());
    let mut chars = source.char_indices().peekable();

    while let Some((offset, ch)) = chars.next() {
        if ch == '<'
            && offset_in_ranges(offset, html_ranges)
            && let Some(tag_len) = br_tag_len(source, offset)
        {
            output.push(' ');
            let tag_end = offset + tag_len;
            while let Some(&(next_offset, _)) = chars.peek() {
                if next_offset < tag_end {
                    chars.next();
                } else {
                    break;
                }
            }
            continue;
        }

        if ch == '\n' && offset_in_ranges(offset, text_ranges) {
            output.push(' ');
            while let Some(&(next_offset, next_ch)) = chars.peek() {
                if (next_ch == ' ' || next_ch == '\t') && offset_in_ranges(next_offset, text_ranges)
                {
                    chars.next();
                } else {
                    break;
                }
            }
            continue;
        }

        output.push(ch);
    }

    output
}

/// Return the byte length of a `<br>`-style tag starting at `offset`.
///
/// ### Arguments
/// - `source`: The original Markdown text
/// - `offset`: The byte offset of the candidate `<`
///
/// ### Returns
/// - `Some(usize)`: The tag length, from `<` through the closing `>` inclusive
/// - `None`: When no `<br>` tag starts at `offset`
fn br_tag_len(source: &str, offset: usize) -> Option<usize> {
    let bytes = source.as_bytes().get(offset..)?;
    if bytes.len() < 4
        || bytes[0] != b'<'
        || !bytes[1].eq_ignore_ascii_case(&b'b')
        || !bytes[2].eq_ignore_ascii_case(&b'r')
        || !matches!(bytes[3], b'>' | b'/' | b' ' | b'\t' | b'\r' | b'\n')
    {
        return None;
    }
    let close = bytes.iter().position(|&byte| byte == b'>')?;
    Some(close + 1)
}

/// Test whether a byte offset lies within any of the disjoint sorted ranges.
///
/// ### Arguments
/// - `offset`: The byte offset to test
/// - `ranges`: Disjoint ranges sorted by start offset
///
/// ### Returns
/// - `bool`: `true` when `offset` is inside a range (`start <= offset < end`)
fn offset_in_ranges(offset: usize, ranges: &[(usize, usize)]) -> bool {
    ranges
        .binary_search_by(|&(start, end)| {
            if offset < start {
                std::cmp::Ordering::Greater
            } else if offset >= end {
                std::cmp::Ordering::Less
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

#[cfg(test)]
mod tests {
    use super::sanitize_filename;

    #[test]
    fn test_sanitize_normal_filename() {
        assert_eq!(sanitize_filename("document.txt"), "document.txt");
        assert_eq!(sanitize_filename("My File.pdf"), "My File.pdf");
    }

    #[test]
    fn test_sanitize_path_traversal() {
        assert_eq!(sanitize_filename("../../etc/passwd"), "passwd");
        assert_eq!(sanitize_filename("../config.json"), "config.json");
        assert_eq!(sanitize_filename("..\\windows\\system32"), "system32");
        // Bare ".." and "." must also be rejected
        assert_eq!(sanitize_filename(".."), "untitled");
        assert_eq!(sanitize_filename("."), "untitled");
    }

    #[test]
    fn test_sanitize_path_separators() {
        assert_eq!(sanitize_filename("path/to/file.txt"), "file.txt");
        assert_eq!(sanitize_filename("C:\\Users\\file.txt"), "file.txt");
        assert_eq!(
            sanitize_filename("/absolute/path/to/document.pdf"),
            "document.pdf"
        );
    }

    #[test]
    fn test_sanitize_hidden_files() {
        assert_eq!(sanitize_filename(".hidden"), ".hidden");
        assert_eq!(sanitize_filename(".gitignore"), ".gitignore");
        assert_eq!(sanitize_filename("path/to/.hidden"), ".hidden");
    }

    #[test]
    fn test_sanitize_empty_and_whitespace() {
        assert_eq!(sanitize_filename(""), "untitled");
        assert_eq!(sanitize_filename("   "), "untitled");
        assert_eq!(sanitize_filename("/"), "untitled");
        assert_eq!(sanitize_filename("\\"), "untitled");
        assert_eq!(sanitize_filename("///"), "untitled");
    }

    #[test]
    fn test_sanitize_control_characters() {
        assert_eq!(sanitize_filename("file\x00name.txt"), "filename.txt");
        assert_eq!(sanitize_filename("test\nfile.txt"), "testfile.txt");
        assert_eq!(sanitize_filename("doc\r\nument.txt"), "document.txt");
    }

    #[test]
    fn test_sanitize_mixed_issues() {
        assert_eq!(
            sanitize_filename("../../.hidden/path/to/file\x00.txt"),
            "file.txt"
        );
        assert_eq!(
            sanitize_filename("/tmp/.config/app/settings.json"),
            "settings.json"
        );
    }

    #[test]
    fn test_sanitize_unicode() {
        assert_eq!(sanitize_filename("文档.txt"), "文档.txt");
        assert_eq!(sanitize_filename("émoji-😀.txt"), "émoji-😀.txt");
    }

    use super::sanitize_markdown_preview;

    #[test]
    fn test_markdown_collapses_paragraph_soft_breaks() {
        // The real regression: a wrapped prose paragraph stores its soft breaks
        // as `\n` inside one Text node, which must become spaces.
        let source = "line one\nline two\nline three\n";
        assert_eq!(
            sanitize_markdown_preview(source),
            "line one line two line three\n"
        );
    }

    #[test]
    fn test_markdown_collapses_badge_block_newlines() {
        let source = "[![A](a)](x)\n[![B](b)](y)\n";
        assert_eq!(
            sanitize_markdown_preview(source),
            "[![A](a)](x) [![B](b)](y)\n"
        );
    }

    #[test]
    fn test_markdown_collapses_multiline_html_block() {
        let source = "<details>\n<summary>Build</summary>\n\n### Steps\n";
        assert_eq!(
            sanitize_markdown_preview(source),
            "<details> <summary>Build</summary>\n\n### Steps\n"
        );
    }

    #[test]
    fn test_markdown_drops_continuation_indentation() {
        // A newline followed by continuation indentation collapses to one space.
        let source = "alpha\n    beta\n";
        assert_eq!(sanitize_markdown_preview(source), "alpha beta\n");
    }

    #[test]
    fn test_markdown_leaves_single_line_html_blocks() {
        let source = "<div align=\"center\">\n\n<img src=\"a.png\" />\n\n</div>\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_preserves_regular_content() {
        let source = "# Title\n\nSome *markdown* text.\n\n- item\n- item\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_preserves_table_structure() {
        // Table cells carry no newlines; the row-separating newlines live
        // between block nodes and must survive.
        let source = "| a | b |\n|---|---|\n| 1 | 2 |\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_keeps_fenced_code_block_newlines() {
        let source = "```html\n<details>\n<summary>x</summary>\n```\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_keeps_indented_code_block_newlines() {
        // A four-space indented block is a code block, not inline text, so its
        // newlines are preserved.
        let source = "    <div>\n    text\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_no_trailing_newline_is_preserved() {
        assert_eq!(sanitize_markdown_preview("plain"), "plain");
    }

    #[test]
    fn test_markdown_replaces_inline_br_tags() {
        assert_eq!(sanitize_markdown_preview("a<br>b\n"), "a b\n");
        assert_eq!(sanitize_markdown_preview("a<br/>b\n"), "a b\n");
        assert_eq!(sanitize_markdown_preview("a<br />b\n"), "a b\n");
        assert_eq!(sanitize_markdown_preview("a<BR>b\n"), "a b\n");
    }

    #[test]
    fn test_markdown_replaces_br_inside_html_block() {
        assert_eq!(
            sanitize_markdown_preview("<div>a<br>b</div>\n"),
            "<div>a b</div>\n"
        );
    }

    #[test]
    fn test_markdown_keeps_br_in_code_span_literal() {
        let source = "`<br>` is a break\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_keeps_br_in_fenced_code_literal() {
        let source = "```html\n<br>\n```\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }

    #[test]
    fn test_markdown_does_not_touch_non_br_tags() {
        // `<broom>` is not a break tag and must be left alone.
        let source = "<broom>x</broom>\n";
        assert_eq!(sanitize_markdown_preview(source), source);
    }
}
