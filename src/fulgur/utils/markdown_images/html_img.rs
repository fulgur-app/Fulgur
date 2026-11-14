use super::rewrite::local_path_to_file_url;
use std::path::Path;

/// Rewrite `src` attributes of `<img>` tags inside a raw HTML fragment.
///
/// ### Arguments
/// - `value`: The raw HTML fragment (an `Html` node's text).
/// - `base_dir`: The directory used to resolve relative paths.
///
/// ### Returns
/// - `Some(String)`: The fragment with at least one local `src` rewritten.
/// - `None`: No `<img>` tag carried a rewritable local `src`.
pub(super) fn rewrite_html_img_src(value: &str, base_dir: Option<&Path>) -> Option<String> {
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

#[cfg(test)]
mod tests {
    use super::super::test_helpers::{base, expected_url, rewritten};

    #[test]
    fn rewrites_html_img_src() {
        let out = rewritten(
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
        let out = rewritten(r"<img src='logo.png' alt='x'>", Some(&base()));
        assert_eq!(
            out,
            format!(r"<img src='{}' alt='x'>", expected_url("logo.png"))
        );
    }

    #[test]
    fn leaves_remote_html_img_untouched() {
        let source = r#"<img src="https://example.com/a.png" />"#;
        assert_eq!(rewritten(source, Some(&base())), source);
    }

    #[test]
    fn rewrites_uppercase_html_img_src() {
        let out = rewritten(r#"<IMG SRC="logo.png">"#, Some(&base()));
        assert_eq!(out, format!(r#"<IMG SRC="{}">"#, expected_url("logo.png")));
    }

    #[cfg(windows)]
    #[test]
    fn leaves_unc_html_img_untouched() {
        let source = r#"<img src="\\attacker\share\x.png" />"#;
        assert_eq!(rewritten(source, Some(&base())), source);
    }
}
