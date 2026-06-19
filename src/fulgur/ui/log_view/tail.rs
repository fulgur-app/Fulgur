//! Pure path, text, and file helpers for the log view (no UI or `Fulgur`).

use std::io::{Read, Seek, SeekFrom};
use std::path::Path;

/// Return whether the log-view toggle should be offered for a file path.
///
/// ### Arguments
/// - `path`: The local file path of the tab
///
/// ### Returns
/// - `bool`: `true` for `log`, `txt`, `out`, and `err` extensions
pub fn log_toggle_available(path: &Path) -> bool {
    matches!(
        extension_lowercase(path).as_deref(),
        Some("log" | "txt" | "out" | "err")
    )
}

/// Return whether a file should open directly in log view by default.
///
/// ### Arguments
/// - `path`: The local file path of the tab
///
/// ### Returns
/// - `bool`: `true` only for the `log` extension
pub fn opens_as_log_by_default(path: &Path) -> bool {
    extension_lowercase(path).as_deref() == Some("log")
}

/// Return the lowercased file extension of a path, if any.
///
/// ### Arguments
/// - `path`: The path to inspect
///
/// ### Returns
/// - `Some(String)`: The lowercased extension
/// - `None`: If the path has no extension
fn extension_lowercase(path: &Path) -> Option<String> {
    path.extension()
        .and_then(|ext| ext.to_str())
        .map(str::to_ascii_lowercase)
}

/// Trim a buffer so that only the last `max_lines` newline-terminated lines
/// (plus any trailing partial line) remain.
///
/// ### Arguments
/// - `buffer`: The full buffer text
/// - `max_lines`: The maximum number of lines to keep
///
/// ### Returns
/// - `(String, bool)`: The trimmed buffer and whether any lines were dropped
pub fn trim_to_last_lines(buffer: String, max_lines: usize) -> (String, bool) {
    let newline_count = buffer.matches('\n').count();
    if newline_count <= max_lines {
        return (buffer, false);
    }
    let lines_to_drop = newline_count - max_lines;
    let mut seen = 0;
    let mut cut = 0;
    for (idx, byte) in buffer.bytes().enumerate() {
        if byte == b'\n' {
            seen += 1;
            if seen == lines_to_drop {
                cut = idx + 1;
                break;
            }
        }
    }
    (buffer[cut..].to_string(), true)
}

/// Read newly appended bytes from a file beyond a known offset.
///
/// ### Arguments
/// - `path`: The file to read
/// - `offset`: The byte offset already consumed
///
/// ### Returns
/// - `Some((String, u64, bool))`: The decoded new text, the new offset, and
///   whether the file was truncated/rotated (offset reset to a full reread)
/// - `None`: If the file could not be stat-ed or read
pub(super) fn read_new_log_bytes(path: &Path, offset: u64) -> Option<(String, u64, bool)> {
    let len = std::fs::metadata(path).ok()?.len();
    if len == offset {
        return Some((String::new(), offset, false));
    }
    if len < offset {
        let bytes = std::fs::read(path).ok()?;
        let new_offset = bytes.len() as u64;
        return Some((
            String::from_utf8_lossy(&bytes).into_owned(),
            new_offset,
            true,
        ));
    }
    let mut file = std::fs::File::open(path).ok()?;
    file.seek(SeekFrom::Start(offset)).ok()?;
    let to_read = len - offset;
    let mut buf = Vec::with_capacity(usize::try_from(to_read).unwrap_or(0));
    file.take(to_read).read_to_end(&mut buf).ok()?;
    let new_offset = offset + buf.len() as u64;
    Some((
        String::from_utf8_lossy(&buf).into_owned(),
        new_offset,
        false,
    ))
}

#[cfg(test)]
mod tests {
    use super::{
        log_toggle_available, opens_as_log_by_default, read_new_log_bytes, trim_to_last_lines,
    };
    use crate::fulgur::ui::log_view::LOG_LINE_CAP;
    use std::io::Write;
    use std::path::Path;

    #[test]
    fn test_log_toggle_available_for_supported_extensions() {
        assert!(log_toggle_available(Path::new("server.log")));
        assert!(log_toggle_available(Path::new("notes.txt")));
        assert!(log_toggle_available(Path::new("build.out")));
        assert!(log_toggle_available(Path::new("build.err")));
        assert!(log_toggle_available(Path::new("SERVER.LOG")));
    }

    #[test]
    fn test_log_toggle_unavailable_for_other_extensions() {
        assert!(!log_toggle_available(Path::new("main.rs")));
        assert!(!log_toggle_available(Path::new("data.csv")));
        assert!(!log_toggle_available(Path::new("noextension")));
    }

    #[test]
    fn test_opens_as_log_by_default_only_for_log() {
        assert!(opens_as_log_by_default(Path::new("server.log")));
        assert!(opens_as_log_by_default(Path::new("SERVER.LOG")));
        assert!(!opens_as_log_by_default(Path::new("notes.txt")));
        assert!(!opens_as_log_by_default(Path::new("build.out")));
    }

    #[test]
    fn test_trim_keeps_all_when_under_cap() {
        let buffer = "a\nb\nc\n".to_string();
        let (trimmed, dropped) = trim_to_last_lines(buffer.clone(), 10);
        assert_eq!(trimmed, buffer);
        assert!(!dropped);
    }

    #[test]
    fn test_trim_drops_front_lines_over_cap() {
        let buffer = "l1\nl2\nl3\nl4\nl5\n".to_string();
        let (trimmed, dropped) = trim_to_last_lines(buffer, 2);
        assert_eq!(trimmed, "l4\nl5\n");
        assert!(dropped);
    }

    #[test]
    fn test_trim_keeps_trailing_partial_line() {
        let buffer = "l1\nl2\nl3\npartial".to_string();
        let (trimmed, dropped) = trim_to_last_lines(buffer, 1);
        assert_eq!(trimmed, "l3\npartial");
        assert!(dropped);
    }

    #[test]
    fn test_trim_at_exact_cap_keeps_everything() {
        use std::fmt::Write as _;
        let mut buffer = String::new();
        for index in 0..LOG_LINE_CAP {
            let _ = writeln!(buffer, "line {index}");
        }
        let original = buffer.clone();
        let (trimmed, dropped) = trim_to_last_lines(buffer, LOG_LINE_CAP);
        assert_eq!(trimmed, original);
        assert!(!dropped);
    }

    #[test]
    fn test_read_new_log_bytes_returns_only_appended_text() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("tail.log");
        std::fs::write(&path, "line1\n").expect("write seed");
        let offset = std::fs::metadata(&path).expect("metadata").len();

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open append");
        file.write_all(b"line2\n").expect("append");
        drop(file);

        let (text, new_offset, truncated) =
            read_new_log_bytes(&path, offset).expect("read new bytes");
        assert_eq!(text, "line2\n");
        assert_eq!(new_offset, offset + 6);
        assert!(!truncated);
    }

    #[test]
    fn test_read_new_log_bytes_reports_no_change_when_unchanged() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("idle.log");
        std::fs::write(&path, "content\n").expect("write");
        let offset = std::fs::metadata(&path).expect("metadata").len();

        let (text, new_offset, truncated) =
            read_new_log_bytes(&path, offset).expect("read new bytes");
        assert!(text.is_empty());
        assert_eq!(new_offset, offset);
        assert!(!truncated);
    }

    #[test]
    fn test_read_new_log_bytes_resets_on_truncation() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rotated.log");
        std::fs::write(&path, "old long content\n").expect("write seed");
        let offset = std::fs::metadata(&path).expect("metadata").len();

        // Truncate/rotate: the file is now shorter than the consumed offset.
        std::fs::write(&path, "fresh\n").expect("truncate");

        let (text, new_offset, truncated) =
            read_new_log_bytes(&path, offset).expect("read new bytes");
        assert_eq!(text, "fresh\n");
        assert_eq!(new_offset, 6);
        assert!(truncated);
    }
}
