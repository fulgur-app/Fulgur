use crate::fulgur::{Fulgur, tab::Tab};
use gpui::{Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::Path;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{Duration, SystemTime};

/// Prefix shared by every temporary HTML file produced by the print flow.
const PRINT_FILE_PREFIX: &str = "fulgur_print_";

/// Extension shared by every temporary HTML file produced by the print flow.
const PRINT_FILE_EXTENSION: &str = ".html";

/// Minimum age before a leftover print file is considered abandoned.
const PRINT_FILE_MAX_AGE: Duration = Duration::from_hours(1);

/// Per-process counter making each print file name unique.
static PRINT_FILE_COUNTER: AtomicU64 = AtomicU64::new(0);

impl Fulgur {
    /// Open the native OS print dialog for the current document
    ///
    /// Writes the active tab's content to a temporary HTML file and opens it with
    /// the system's default browser, which automatically triggers the native print dialog.
    /// This approach works cross-platform without requiring OS-specific print APIs.
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn print_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_tab_index) = self.active_tab_index(cx) else {
            return;
        };
        let (title, content) = match self.tabs[active_tab_index].read(cx) {
            Tab::Editor(editor_tab) => {
                let title = editor_tab.title.clone();
                let content = editor_tab.content.read(cx).text().to_string();
                (title, content)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let escaped_content = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let escaped_title = title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{escaped_title}</title>
<style>
  body {{ margin: 0; padding: 1em; font-family: monospace; white-space: pre-wrap; word-wrap: break-word; }}
  @media print {{ body {{ margin: 0; }} }}
</style>
</head>
<body>{escaped_content}</body>
<script>window.onload = function() {{ window.print(); }};</script>
</html>"#,
        );
        let temp_dir = std::env::temp_dir();
        cleanup_stale_print_files(&temp_dir, SystemTime::now(), PRINT_FILE_MAX_AGE);
        let temp_path = temp_dir.join(next_print_file_name());
        if let Err(e) = std::fs::write(&temp_path, html.as_bytes()) {
            log::error!("Failed to write print temp file: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to prepare print: {e}")),
                ),
                cx,
            );
            return;
        }
        if let Err(e) = open::that(&temp_path) {
            log::error!("Failed to open print file: {e}");
            let _ = std::fs::remove_file(&temp_path);
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to open print dialog: {e}")),
                ),
                cx,
            );
        }
    }
}

/// Build a print file name unique to this process and to this print request.
///
/// ### Returns
/// - `String`: The file name, for example `fulgur_print_4231_0.html`
fn next_print_file_name() -> String {
    let sequence = PRINT_FILE_COUNTER.fetch_add(1, Ordering::Relaxed);
    format!(
        "{PRINT_FILE_PREFIX}{}_{sequence}{PRINT_FILE_EXTENSION}",
        std::process::id()
    )
}

/// Remove print files left behind by earlier prints or by crashed runs.
///
/// ### Arguments
/// - `dir`: The directory holding the temporary print files
/// - `now`: The reference instant that file ages are measured against
/// - `max_age`: The minimum age a file must reach before it is removed
fn cleanup_stale_print_files(dir: &Path, now: SystemTime, max_age: Duration) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !is_print_file(&path) {
            continue;
        }
        let Ok(modified) = entry.metadata().and_then(|metadata| metadata.modified()) else {
            continue;
        };
        let Ok(age) = now.duration_since(modified) else {
            continue;
        };
        if age < max_age {
            continue;
        }
        if let Err(e) = std::fs::remove_file(&path) {
            log::warn!(
                "Failed to remove stale print file '{}': {e}",
                path.display()
            );
        } else {
            log::debug!("Removed stale print file '{}'", path.display());
        }
    }
}

/// Report whether a path names a temporary print file produced by Fulgur.
///
/// ### Arguments
/// - `path`: The path to test
///
/// ### Returns
/// - `bool`: `true` when the file name matches the print file naming scheme
fn is_print_file(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| {
            name.starts_with(PRINT_FILE_PREFIX) && name.ends_with(PRINT_FILE_EXTENSION)
        })
}

#[cfg(test)]
mod tests {
    use super::{
        PRINT_FILE_EXTENSION, PRINT_FILE_MAX_AGE, PRINT_FILE_PREFIX, cleanup_stale_print_files,
    };
    use std::fs;
    use std::time::{Duration, SystemTime};
    use tempfile::TempDir;

    #[test]
    fn next_print_file_name_is_unique_per_call() {
        let first = super::next_print_file_name();
        let second = super::next_print_file_name();

        assert_ne!(first, second, "successive print files must not collide");
        assert!(first.starts_with(PRINT_FILE_PREFIX));
        assert!(first.ends_with(PRINT_FILE_EXTENSION));
    }

    #[test]
    fn cleanup_removes_old_print_files_only() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        let stale = temp_dir.path().join("fulgur_print_1234_0.html");
        fs::write(&stale, b"stale").expect("failed to write stale print file");
        let unrelated = temp_dir.path().join("report.html");
        fs::write(&unrelated, b"unrelated").expect("failed to write unrelated file");

        cleanup_stale_print_files(temp_dir.path(), SystemTime::now(), Duration::ZERO);

        assert!(!stale.exists(), "stale print file should be removed");
        assert!(unrelated.exists(), "unrelated file should be kept");
    }

    #[test]
    fn cleanup_keeps_recent_print_files() {
        let temp_dir = TempDir::new().expect("failed to create temp dir");

        let recent = temp_dir.path().join("fulgur_print_1234_1.html");
        fs::write(&recent, b"recent").expect("failed to write recent print file");

        cleanup_stale_print_files(temp_dir.path(), SystemTime::now(), PRINT_FILE_MAX_AGE);

        assert!(
            recent.exists(),
            "a print file the browser may still be reading should be kept"
        );
    }
}
