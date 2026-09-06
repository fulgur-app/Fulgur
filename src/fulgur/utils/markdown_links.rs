//! Classify Markdown preview link targets so activation can be routed.

use http_client::Url;
use std::path::{Path, PathBuf};

/// What a link in the Markdown preview points at.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MarkdownLinkTarget {
    /// A local file that should be opened in an editor tab.
    LocalFile(PathBuf),
    /// A remote or non-file URL that belongs to the system handler.
    External(String),
    /// An in-document anchor, which the preview cannot navigate to.
    Anchor,
}

/// Resolve a Markdown link reference into an actionable target.
///
/// ### Arguments
/// - `reference`: The raw link destination taken from the rendered document.
/// - `base_dir`: The directory of the previewed file, used to resolve relative
///   references. When `None`, only absolute paths resolve to a local file.
///
/// ### Returns
/// - `Some(MarkdownLinkTarget)`: The classified target.
/// - `None`: The reference is empty or names a local path that cannot be
///   resolved without a base directory.
#[must_use]
pub fn resolve_markdown_link(
    reference: &str,
    base_dir: Option<&Path>,
) -> Option<MarkdownLinkTarget> {
    let trimmed = reference.trim();
    if trimmed.is_empty() {
        return None;
    }
    if trimmed.starts_with('#') {
        return Some(MarkdownLinkTarget::Anchor);
    }
    if trimmed.starts_with("//") {
        return Some(MarkdownLinkTarget::External(format!("https:{trimmed}")));
    }

    match scheme_of(trimmed) {
        Some(scheme) if scheme.eq_ignore_ascii_case("file") => Url::parse(trimmed)
            .ok()?
            .to_file_path()
            .ok()
            .map(MarkdownLinkTarget::LocalFile),
        Some(_) => Some(MarkdownLinkTarget::External(trimmed.to_string())),
        None => resolve_local_path(trimmed, base_dir).map(MarkdownLinkTarget::LocalFile),
    }
}

/// Extract the URL scheme of a reference, if it carries one.
///
/// ### Arguments
/// - `reference`: The trimmed link destination.
///
/// ### Returns
/// - `Some(&str)`: The scheme, without the trailing colon.
/// - `None`: The reference has no scheme. A single-letter prefix is rejected
///   so a Windows drive letter is not mistaken for one.
fn scheme_of(reference: &str) -> Option<&str> {
    let colon = reference.find(':')?;
    let scheme = &reference[..colon];
    if scheme.len() < 2 || !scheme.starts_with(|c: char| c.is_ascii_alphabetic()) {
        return None;
    }
    scheme
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        .then_some(scheme)
}

/// Resolve a schemeless reference to a filesystem path.
///
/// ### Arguments
/// - `reference`: The trimmed link destination, possibly with a fragment.
/// - `base_dir`: The directory used to resolve a relative reference.
///
/// ### Returns
/// - `Some(PathBuf)`: The resolved path, percent-decoded and normalized.
/// - `None`: The reference is only a fragment, or is relative with no base
///   directory to resolve it against.
fn resolve_local_path(reference: &str, base_dir: Option<&Path>) -> Option<PathBuf> {
    let target = reference.split('#').next()?.trim();
    if target.is_empty() {
        return None;
    }

    if let Some(base_dir) = base_dir
        && let Ok(base_url) = Url::from_directory_path(base_dir)
        && let Ok(joined) = base_url.join(target)
        && let Ok(path) = joined.to_file_path()
    {
        return Some(path);
    }

    let path = Path::new(target);
    path.is_absolute().then(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn base() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\docs")
        } else {
            PathBuf::from("/docs")
        }
    }

    fn local(reference: &str) -> Option<PathBuf> {
        match resolve_markdown_link(reference, Some(&base())) {
            Some(MarkdownLinkTarget::LocalFile(path)) => Some(path),
            _ => None,
        }
    }

    #[test]
    fn resolves_relative_markdown_file() {
        assert_eq!(local("notes/other.md"), Some(base().join("notes/other.md")));
    }

    #[test]
    fn resolves_parent_relative_reference() {
        let expected = base().parent().unwrap().join("readme.md");
        assert_eq!(local("../readme.md"), Some(expected));
    }

    #[test]
    fn resolves_dot_relative_reference() {
        assert_eq!(local("./other.md"), Some(base().join("other.md")));
    }

    #[test]
    fn percent_decodes_escaped_spaces() {
        assert_eq!(local("my%20notes.md"), Some(base().join("my notes.md")));
    }

    #[test]
    fn strips_the_fragment_from_a_local_reference() {
        assert_eq!(local("other.md#section"), Some(base().join("other.md")));
    }

    #[test]
    fn resolves_absolute_path_without_base_dir() {
        let absolute = base().join("other.md");
        let reference = absolute.to_string_lossy().to_string();
        assert_eq!(
            resolve_markdown_link(&reference, None),
            Some(MarkdownLinkTarget::LocalFile(absolute))
        );
    }

    #[test]
    fn resolves_file_url_to_a_local_path() {
        let absolute = base().join("other.md");
        let url = Url::from_file_path(&absolute).unwrap().to_string();
        assert_eq!(
            resolve_markdown_link(&url, None),
            Some(MarkdownLinkTarget::LocalFile(absolute))
        );
    }

    #[test]
    fn classifies_http_links_as_external() {
        assert_eq!(
            resolve_markdown_link("https://example.com/a", Some(&base())),
            Some(MarkdownLinkTarget::External(
                "https://example.com/a".to_string()
            ))
        );
    }

    #[test]
    fn classifies_mailto_links_as_external() {
        assert_eq!(
            resolve_markdown_link("mailto:someone@example.com", None),
            Some(MarkdownLinkTarget::External(
                "mailto:someone@example.com".to_string()
            ))
        );
    }

    #[test]
    fn upgrades_protocol_relative_links_to_https() {
        assert_eq!(
            resolve_markdown_link("//example.com/a", None),
            Some(MarkdownLinkTarget::External(
                "https://example.com/a".to_string()
            ))
        );
    }

    #[test]
    fn classifies_in_document_anchors() {
        assert_eq!(
            resolve_markdown_link("#heading", Some(&base())),
            Some(MarkdownLinkTarget::Anchor)
        );
    }

    #[test]
    fn relative_reference_without_base_dir_is_unresolvable() {
        assert_eq!(resolve_markdown_link("other.md", None), None);
    }

    #[test]
    fn empty_reference_is_unresolvable() {
        assert_eq!(resolve_markdown_link("   ", Some(&base())), None);
    }

    #[test]
    fn single_letter_scheme_is_treated_as_a_drive_letter() {
        // Only meaningful on Windows, but the classification must not send a
        // path to the system browser on any platform.
        assert!(!matches!(
            resolve_markdown_link(r"C:\docs\other.md", None),
            Some(MarkdownLinkTarget::External(_))
        ));
    }
}
