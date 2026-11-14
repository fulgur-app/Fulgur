use super::html_img::rewrite_html_img_src;
use crate::fulgur::utils::http::is_network_or_verbatim_path;
use gpui_kit::http_client::Url;
use std::borrow::Cow;
use std::ops::Range;
use std::path::Path;

/// Rewrite local image references in `source` to absolute `file://` URLs.
///
/// ### Arguments
/// - `source`: The Markdown text about to be rendered in the preview.
/// - `base_dir`: The directory of the source file, used to resolve relative
///   image paths. When `None`, relative paths are left unchanged (they cannot
///   be resolved) while absolute paths are still converted.
///
/// ### Returns
/// - `RewrittenMarkdown`: The Markdown with local image references rewritten,
///   borrowing the source unchanged when it holds no rewritable image (or
///   fails to parse), with the offsets mapping it back to `source`.
#[must_use]
pub fn rewrite_markdown_image_paths<'a>(
    source: &'a str,
    base_dir: Option<&Path>,
) -> RewrittenMarkdown<'a> {
    let unchanged = RewrittenMarkdown {
        text: Cow::Borrowed(source),
        offsets: RewriteOffsets::default(),
    };
    if !may_reference_image(source) {
        return unchanged;
    }
    let Ok(ast) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return unchanged;
    };

    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    collect_image_replacements(&ast, base_dir, &mut replacements);
    if replacements.is_empty() {
        return unchanged;
    }

    let (text, offsets) = apply_replacements(source, &mut replacements);
    RewrittenMarkdown {
        text: Cow::Owned(text),
        offsets,
    }
}

/// Markdown whose local image references were rewritten for the preview.
pub struct RewrittenMarkdown<'a> {
    /// The rewritten Markdown, borrowed when nothing was rewritten.
    pub text: Cow<'a, str>,
    /// Maps byte ranges of `text` back to the source it was rewritten from.
    pub offsets: RewriteOffsets,
}

/// A source span the rewrite replaced, and the length of its replacement.
#[derive(Debug, Clone, PartialEq, Eq)]
struct RewrittenSpan {
    source: Range<usize>,
    rewritten_len: usize,
}

/// Translates byte ranges of rewritten Markdown back to its source.
///
/// The rewrite only replaces image references, so any text outside them sits
/// at the same position in the source, shifted by the length the replacements
/// before it added or removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RewriteOffsets {
    /// Replaced spans in ascending, non-overlapping source order.
    spans: Vec<RewrittenSpan>,
}

impl RewriteOffsets {
    /// Map a byte range of the rewritten text to the same text in the source.
    ///
    /// ### Arguments
    /// - `range`: A byte range of the rewritten text
    ///
    /// ### Returns
    /// - `Some(Range<usize>)`: The range in the source holding the same bytes
    /// - `None`: The range overlaps a rewritten image reference, which has no
    ///   byte-for-byte counterpart in the source
    #[must_use]
    pub fn to_source(&self, range: Range<usize>) -> Option<Range<usize>> {
        // Bytes the replacements before `range` added to the rewritten text,
        // negative when they shortened it.
        let mut shift: isize = 0;
        for span in &self.spans {
            let rewritten_start = span.source.start.checked_add_signed(shift)?;
            if range.end <= rewritten_start {
                break;
            }
            if range.start < rewritten_start + span.rewritten_len {
                return None;
            }
            shift += span.rewritten_len.cast_signed() - span.source.len().cast_signed();
        }
        Some(range.start.checked_add_signed(-shift)?..range.end.checked_add_signed(-shift)?)
    }
}

/// Cheaply tell whether `source` can contain an image worth parsing for.
///
/// ### Arguments
/// - `source`: The Markdown text about to be rendered in the preview.
///
/// ### Returns
/// - `bool`: `true` when `source` contains a Markdown image opener (`![`) or an
///   HTML `<img` tag in any case; `false` when a full parse cannot find one.
fn may_reference_image(source: &str) -> bool {
    source.contains("![")
        || source
            .as_bytes()
            .windows(4)
            .any(|window| window.eq_ignore_ascii_case(b"<img"))
}

/// Walk the AST, collecting `(start, end, replacement)` edits for local images.
///
/// ### Arguments
/// - `node`: The current AST node being visited.
/// - `base_dir`: The directory used to resolve relative paths.
/// - `replacements`: Accumulator for the byte ranges and their new text.
fn collect_image_replacements(
    node: &markdown::mdast::Node,
    base_dir: Option<&Path>,
    replacements: &mut Vec<(usize, usize, String)>,
) {
    use markdown::mdast::Node;

    match node {
        Node::Image(image) => {
            if let Some(position) = &image.position
                && let Some(new_url) = local_path_to_file_url(&image.url, base_dir)
            {
                let title = image
                    .title
                    .as_ref()
                    .map_or_default(|title| format!(" \"{title}\""));
                let rebuilt = format!("![{}]({}{})", image.alt, new_url, title);
                replacements.push((position.start.offset, position.end.offset, rebuilt));
            }
        }
        Node::Html(html) => {
            if let Some(position) = &html.position
                && let Some(new_value) = rewrite_html_img_src(&html.value, base_dir)
            {
                replacements.push((position.start.offset, position.end.offset, new_value));
            }
        }
        _ => {}
    }

    if let Some(children) = node.children() {
        for child in children {
            collect_image_replacements(child, base_dir, replacements);
        }
    }
}

/// Convert a local image path to an absolute `file://` URL.
///
/// ### Arguments
/// - `url`: The raw image URL from the Markdown or HTML source.
/// - `base_dir`: The directory used to resolve a relative path.
///
/// ### Returns
/// - `Some(String)`: A `file://` URL for a resolvable local path.
/// - `None`: The URL is remote, already a URL, a network or verbatim path, or a
///   relative path that cannot be resolved without a base directory.
pub(super) fn local_path_to_file_url(url: &str, base_dir: Option<&Path>) -> Option<String> {
    let trimmed = url.trim();
    if trimmed.is_empty() || is_non_local_reference(trimmed) {
        return None;
    }

    let path = Path::new(trimmed);
    let absolute = if path.is_absolute() {
        path.to_path_buf()
    } else {
        base_dir?.join(path)
    };
    if is_network_or_verbatim_path(&absolute) {
        return None;
    }

    Url::from_file_path(&absolute)
        .ok()
        .map(|url| url.to_string())
}

/// Return whether a reference should be left untouched (not a local path).
///
/// ### Arguments
/// - `reference`: The trimmed URL to classify.
///
/// ### Returns
/// - `true`: The reference is remote, a data URI, protocol-relative, or an
///   existing `file://` URL.
/// - `false`: The reference is a local filesystem path.
fn is_non_local_reference(reference: &str) -> bool {
    let lower = reference.to_ascii_lowercase();
    lower.starts_with("http://")
        || lower.starts_with("https://")
        || lower.starts_with("file://")
        || lower.starts_with("data:")
        || lower.starts_with("//")
}

/// Apply `(start, end, replacement)` edits to `source`, non-overlapping.
///
/// ### Arguments
/// - `source`: The original text.
/// - `replacements`: Byte-range edits; reordered in place by ascending start.
///
/// ### Returns
/// - `(String, RewriteOffsets)`: The edited text, and the offsets mapping it
///   back to `source` through the edits actually applied.
fn apply_replacements(
    source: &str,
    replacements: &mut [(usize, usize, String)],
) -> (String, RewriteOffsets) {
    replacements.sort_unstable_by_key(|&(start, _, _)| start);

    let mut output = String::with_capacity(source.len());
    let mut offsets = RewriteOffsets::default();
    let mut cursor = 0;
    for (start, end, replacement) in replacements.iter() {
        // Skip any edit that would overlap an already-applied one.
        if *start < cursor {
            continue;
        }
        output.push_str(&source[cursor..*start]);
        output.push_str(replacement);
        offsets.spans.push(RewrittenSpan {
            source: *start..*end,
            rewritten_len: replacement.len(),
        });
        cursor = *end;
    }
    output.push_str(&source[cursor..]);
    (output, offsets)
}

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{base, expected_url, rewritten};
    use super::{may_reference_image, rewrite_markdown_image_paths};
    use std::borrow::Cow;

    #[test]
    fn rewrites_relative_markdown_image() {
        let out = rewritten("![alt](img/logo.png)", Some(&base()));
        assert_eq!(out, format!("![alt]({})", expected_url("img/logo.png")));
    }

    #[test]
    fn preserves_markdown_title() {
        let out = rewritten("![a](logo.png \"Title\")", Some(&base()));
        assert_eq!(out, format!("![a]({} \"Title\")", expected_url("logo.png")));
    }

    #[test]
    fn rewrites_absolute_markdown_image_without_base() {
        let absolute = base().join("logo.png");
        let source = format!("![a]({})", absolute.display());
        let out = rewritten(&source, None);
        assert_eq!(out, format!("![a]({})", expected_url("logo.png")));
    }

    #[test]
    fn leaves_remote_images_untouched() {
        let source = "![a](https://example.com/logo.png)";
        assert_eq!(rewritten(source, Some(&base())), source);
    }

    #[test]
    fn leaves_data_uri_untouched() {
        let source = "![a](data:image/png;base64,AAAA)";
        assert_eq!(rewritten(source, Some(&base())), source);
    }

    #[test]
    fn relative_path_without_base_is_unchanged() {
        let source = "![a](logo.png)";
        assert_eq!(rewritten(source, None), source);
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let source = "# Heading\n\nSome text with no images.";
        assert_eq!(rewritten(source, Some(&base())), source);
    }

    #[test]
    fn borrows_source_without_image_markers() {
        let source = "[a link](docs/readme.md) and `src/main.rs`";
        assert!(matches!(rewritten(source, Some(&base())), Cow::Borrowed(_)));
    }

    #[test]
    fn offsets_map_text_after_rewritten_images_back_to_the_source() {
        let source = "![a](a.png)\n\n<img src=\"b.png\">\n\n- [ ] Task\n";
        let rewrite = rewrite_markdown_image_paths(source, Some(&base()));
        assert_ne!(rewrite.text, source, "both images are rewritten");

        let rewritten_marker = rewrite.text.find("[ ]").unwrap();
        let source_marker = source.find("[ ]").unwrap();
        assert_eq!(
            rewrite
                .offsets
                .to_source(rewritten_marker..rewritten_marker + 3),
            Some(source_marker..source_marker + 3)
        );
    }

    #[test]
    fn offsets_keep_text_before_any_rewrite_in_place() {
        let source = "- [x] Done\n\n![a](a.png)\n";
        let rewrite = rewrite_markdown_image_paths(source, Some(&base()));
        assert_eq!(rewrite.offsets.to_source(2..5), Some(2..5));
    }

    #[test]
    fn offsets_refuse_ranges_inside_a_rewritten_image() {
        let source = "![a](a.png) after";
        let rewrite = rewrite_markdown_image_paths(source, Some(&base()));
        let url_start = rewrite.text.find("file:").unwrap();
        assert_eq!(rewrite.offsets.to_source(url_start..url_start + 3), None);
    }

    #[test]
    fn offsets_are_the_identity_without_a_rewrite() {
        let rewrite = rewrite_markdown_image_paths("- [ ] Task", Some(&base()));
        assert_eq!(rewrite.offsets.to_source(2..5), Some(2..5));
    }

    #[test]
    fn may_reference_image_detects_markers() {
        assert!(may_reference_image("![a](b.png)"));
        assert!(may_reference_image("<Img src='b.png'>"));
        assert!(!may_reference_image("! [not an image] <im g>"));
    }

    #[cfg(windows)]
    #[test]
    fn leaves_unc_markdown_image_untouched() {
        // CommonMark unescapes `\\`, so the parsed URL is `\\attacker\share\x.png`.
        let source = r"![a](\\\\attacker\\share\\x.png)";
        assert_eq!(rewritten(source, Some(&base())), source);
    }
}
