//! Resolve Markdown preview images and require permission for remote content.

mod html_img;
mod image_source;
mod rewrite;

pub use image_source::preview_image_source;
pub use rewrite::{RewriteOffsets, RewrittenMarkdown, rewrite_markdown_image_paths};

#[cfg(test)]
mod test_helpers {
    use super::rewrite_markdown_image_paths;
    use gpui_kit::http_client::Url;
    use std::borrow::Cow;
    use std::path::{Path, PathBuf};

    pub(super) fn rewritten<'a>(source: &'a str, base_dir: Option<&Path>) -> Cow<'a, str> {
        rewrite_markdown_image_paths(source, base_dir).text
    }

    pub(super) fn base() -> PathBuf {
        if cfg!(windows) {
            PathBuf::from(r"C:\docs")
        } else {
            PathBuf::from("/docs")
        }
    }

    pub(super) fn expected_url(relative: &str) -> String {
        Url::from_file_path(base().join(relative))
            .unwrap()
            .to_string()
    }
}
