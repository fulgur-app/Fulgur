//! Rewrite local image references in Markdown so the preview can load them.

use http_client::Url;
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
/// - `String`: The Markdown with local image references rewritten; the source
///   unchanged when it parses to no rewritable image (or fails to parse).
#[must_use]
pub fn rewrite_markdown_image_paths(source: &str, base_dir: Option<&Path>) -> String {
    let Ok(ast) = markdown::to_mdast(source, &markdown::ParseOptions::gfm()) else {
        return source.to_string();
    };

    let mut replacements: Vec<(usize, usize, String)> = Vec::new();
    collect_image_replacements(&ast, base_dir, &mut replacements);
    if replacements.is_empty() {
        return source.to_string();
    }

    apply_replacements(source, &mut replacements)
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
                    .map(|title| format!(" \"{title}\""))
                    .unwrap_or_default();
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

/// Rewrite `src` attributes of `<img>` tags inside a raw HTML fragment.
///
/// ### Arguments
/// - `value`: The raw HTML fragment (an `Html` node's text).
/// - `base_dir`: The directory used to resolve relative paths.
///
/// ### Returns
/// - `Some(String)`: The fragment with at least one local `src` rewritten.
/// - `None`: No `<img>` tag carried a rewritable local `src`.
fn rewrite_html_img_src(value: &str, base_dir: Option<&Path>) -> Option<String> {
    let mut output = String::with_capacity(value.len());
    let mut changed = false;
    let mut rest = value;

    while let Some(src_start) = find_img_src(rest) {
        let attr_value_start = src_start.attr_value_start;
        output.push_str(&rest[..attr_value_start]);

        let quote = match rest[attr_value_start..].chars().next() {
            Some(quote @ ('"' | '\'')) => quote,
            // Unquoted or empty src value: not handled, advance one char so the
            // scan makes progress instead of looping on the same `src=`.
            other => {
                if let Some(other) = other {
                    output.push(other);
                    rest = &rest[attr_value_start + other.len_utf8()..];
                } else {
                    rest = &rest[attr_value_start..];
                }
                continue;
            }
        };

        let value_body_start = attr_value_start + 1;
        let Some(value_len) = rest[value_body_start..].find(quote) else {
            // Unterminated quote; emit the remainder untouched.
            output.push_str(&rest[attr_value_start..]);
            return changed.then_some(output);
        };
        let url = &rest[value_body_start..value_body_start + value_len];

        output.push(quote);
        if let Some(new_url) = local_path_to_file_url(url, base_dir) {
            output.push_str(&new_url);
            changed = true;
        } else {
            output.push_str(url);
        }
        output.push(quote);
        rest = &rest[value_body_start + value_len + 1..];
    }

    output.push_str(rest);
    changed.then_some(output)
}

/// Location of an `src=` attribute value within an HTML fragment.
struct SrcMatch {
    attr_value_start: usize,
}

/// Find the next `<img>` `src=` attribute in an HTML fragment.
///
/// ### Arguments
/// - `haystack`: The HTML fragment to scan.
///
/// ### Returns
/// - `Some(SrcMatch)`: The position just after the next `src=`.
/// - `None`: No `src=` attribute is present.
fn find_img_src(haystack: &str) -> Option<SrcMatch> {
    let lower = haystack.to_ascii_lowercase();
    let mut search_from = 0;
    while let Some(rel) = lower[search_from..].find("src") {
        let src_pos = search_from + rel;
        let after = src_pos + 3;
        // Allow whitespace between `src` and `=`.
        let mut cursor = after;
        while lower.as_bytes().get(cursor) == Some(&b' ') {
            cursor += 1;
        }
        if lower.as_bytes().get(cursor) == Some(&b'=') {
            cursor += 1;
            while lower.as_bytes().get(cursor) == Some(&b' ') {
                cursor += 1;
            }
            return Some(SrcMatch {
                attr_value_start: cursor,
            });
        }
        search_from = after;
    }
    None
}

/// Convert a local image path to an absolute `file://` URL.
///
/// ### Arguments
/// - `url`: The raw image URL from the Markdown or HTML source.
/// - `base_dir`: The directory used to resolve a relative path.
///
/// ### Returns
/// - `Some(String)`: A `file://` URL for a resolvable local path.
/// - `None`: The URL is remote, already a URL, or a relative path that cannot
///   be resolved without a base directory.
fn local_path_to_file_url(url: &str, base_dir: Option<&Path>) -> Option<String> {
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
/// - `String`: The edited text.
fn apply_replacements(source: &str, replacements: &mut [(usize, usize, String)]) -> String {
    replacements.sort_unstable_by_key(|&(start, _, _)| start);

    let mut output = String::with_capacity(source.len());
    let mut cursor = 0;
    for (start, end, replacement) in replacements.iter() {
        // Skip any edit that would overlap an already-applied one.
        if *start < cursor {
            continue;
        }
        output.push_str(&source[cursor..*start]);
        output.push_str(replacement);
        cursor = *end;
    }
    output.push_str(&source[cursor..]);
    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    fn base() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\docs")
        } else {
            PathBuf::from("/docs")
        }
    }

    fn expected_url(relative: &str) -> String {
        Url::from_file_path(base().join(relative))
            .unwrap()
            .to_string()
    }

    #[test]
    fn rewrites_relative_markdown_image() {
        let out = rewrite_markdown_image_paths("![alt](img/logo.png)", Some(&base()));
        assert_eq!(out, format!("![alt]({})", expected_url("img/logo.png")));
    }

    #[test]
    fn preserves_markdown_title() {
        let out = rewrite_markdown_image_paths("![a](logo.png \"Title\")", Some(&base()));
        assert_eq!(out, format!("![a]({} \"Title\")", expected_url("logo.png")));
    }

    #[test]
    fn rewrites_absolute_markdown_image_without_base() {
        let absolute = base().join("logo.png");
        let source = format!("![a]({})", absolute.display());
        let out = rewrite_markdown_image_paths(&source, None);
        assert_eq!(out, format!("![a]({})", expected_url("logo.png")));
    }

    #[test]
    fn leaves_remote_images_untouched() {
        let source = "![a](https://example.com/logo.png)";
        assert_eq!(rewrite_markdown_image_paths(source, Some(&base())), source);
    }

    #[test]
    fn leaves_data_uri_untouched() {
        let source = "![a](data:image/png;base64,AAAA)";
        assert_eq!(rewrite_markdown_image_paths(source, Some(&base())), source);
    }

    #[test]
    fn rewrites_html_img_src() {
        let out = rewrite_markdown_image_paths(
            r#"<img src="assets/logo.webp" width="128" />"#,
            Some(&base()),
        );
        assert_eq!(
            out,
            format!(
                r#"<img src="{}" width="128" />"#,
                expected_url("assets/logo.webp")
            )
        );
    }

    #[test]
    fn rewrites_single_quoted_html_img_src() {
        let out = rewrite_markdown_image_paths(r"<img src='logo.png' alt='x'>", Some(&base()));
        assert_eq!(
            out,
            format!(r"<img src='{}' alt='x'>", expected_url("logo.png"))
        );
    }

    #[test]
    fn leaves_remote_html_img_untouched() {
        let source = r#"<img src="https://example.com/a.png" />"#;
        assert_eq!(rewrite_markdown_image_paths(source, Some(&base())), source);
    }

    #[test]
    fn relative_path_without_base_is_unchanged() {
        let source = "![a](logo.png)";
        assert_eq!(rewrite_markdown_image_paths(source, None), source);
    }

    #[test]
    fn leaves_plain_text_untouched() {
        let source = "# Heading\n\nSome text with no images.";
        assert_eq!(rewrite_markdown_image_paths(source, Some(&base())), source);
    }
}
