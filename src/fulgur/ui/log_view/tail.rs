//! Pure path, text, and file helpers for the log view (no UI or `Fulgur`).

use std::fs::File;
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

/// Identity of the file object behind a path, used to detect rename-based
/// rotation where the path is re-pointed at a different file.
///
/// On Unix this is the device and inode pair; on Windows it is the volume
/// serial number and file index pair.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogFileIdentity {
    device: u64,
    file_index: u64,
}

impl LogFileIdentity {
    /// Read the identity of an already opened file from its handle.
    ///
    /// ### Arguments
    /// - `file`: The opened file
    ///
    /// ### Returns
    /// - `Some(LogFileIdentity)`: The identity of the file object
    /// - `None`: If the platform could not report it
    #[cfg(unix)]
    fn from_file(file: &File) -> Option<Self> {
        use std::os::unix::fs::MetadataExt;
        let metadata = file.metadata().ok()?;
        Some(Self {
            device: metadata.dev(),
            file_index: metadata.ino(),
        })
    }

    /// Read the identity of an already opened file from its handle.
    ///
    /// ### Arguments
    /// - `file`: The opened file
    ///
    /// ### Returns
    /// - `Some(LogFileIdentity)`: The identity of the file object
    /// - `None`: If the platform could not report it
    #[cfg(windows)]
    fn from_file(file: &File) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows::Win32::Foundation::HANDLE;
        use windows::Win32::Storage::FileSystem::{
            BY_HANDLE_FILE_INFORMATION, GetFileInformationByHandle,
        };
        let mut info = BY_HANDLE_FILE_INFORMATION::default();
        // SAFETY: the handle is owned by `file` and stays valid for the call,
        // and `info` is a properly sized out-parameter.
        unsafe { GetFileInformationByHandle(HANDLE(file.as_raw_handle()), &mut info) }.ok()?;
        Some(Self {
            device: u64::from(info.dwVolumeSerialNumber),
            file_index: (u64::from(info.nFileIndexHigh) << 32) | u64::from(info.nFileIndexLow),
        })
    }
}

/// The current position of a tailed file: its length and identity, both read
/// from a single opened handle.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct LogFilePosition {
    /// The byte length of the file
    pub byte_offset: u64,
    /// The identity of the file object, when the platform reports it
    pub identity: Option<LogFileIdentity>,
}

/// Read the length and identity of the file at a path.
///
/// ### Arguments
/// - `path`: The file to inspect
///
/// ### Returns
/// - `Some(LogFilePosition)`: The file length and identity
/// - `None`: If the file could not be opened or stat-ed
pub(super) fn log_file_position(path: &Path) -> Option<LogFilePosition> {
    let file = File::open(path).ok()?;
    Some(LogFilePosition {
        byte_offset: file.metadata().ok()?.len(),
        identity: LogFileIdentity::from_file(&file),
    })
}

/// A chunk of newly read log bytes and the position it advanced to.
#[derive(Debug, PartialEq, Eq)]
pub(super) struct LogTailChunk {
    /// The decoded new text
    pub text: String,
    /// The consumed position after this read
    pub position: LogFilePosition,
    /// Whether the file was truncated or replaced, so `text` is a full reread
    pub reset: bool,
}

/// Read newly appended bytes from a file beyond a known position.
///
/// ### Arguments
/// - `path`: The file to read
/// - `consumed`: The position already consumed
///
/// ### Returns
/// - `Some(LogTailChunk)`: The new text, the new position, and whether the
///   file was reset (truncated or replaced)
/// - `None`: If the file could not be opened, stat-ed, or read
pub(super) fn read_new_log_bytes(path: &Path, consumed: LogFilePosition) -> Option<LogTailChunk> {
    let mut file = File::open(path).ok()?;
    let len = file.metadata().ok()?.len();
    let identity = LogFileIdentity::from_file(&file);
    let replaced = match (consumed.identity, identity) {
        (Some(known), Some(current)) => known != current,
        _ => false,
    };
    let offset = consumed.byte_offset;
    if replaced || len < offset {
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).ok()?;
        return Some(LogTailChunk {
            text: String::from_utf8_lossy(&bytes).into_owned(),
            position: LogFilePosition {
                byte_offset: bytes.len() as u64,
                identity,
            },
            reset: true,
        });
    }
    if len == offset {
        return Some(LogTailChunk {
            text: String::new(),
            position: LogFilePosition {
                byte_offset: offset,
                identity,
            },
            reset: false,
        });
    }
    file.seek(SeekFrom::Start(offset)).ok()?;
    let to_read = len - offset;
    let mut buf = Vec::with_capacity(usize::try_from(to_read).unwrap_or(0));
    file.take(to_read).read_to_end(&mut buf).ok()?;
    Some(LogTailChunk {
        text: String::from_utf8_lossy(&buf).into_owned(),
        position: LogFilePosition {
            byte_offset: offset + buf.len() as u64,
            identity,
        },
        reset: false,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        LogFilePosition, log_file_position, log_toggle_available, opens_as_log_by_default,
        read_new_log_bytes,
    };
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

    /// Write a seed file and return its consumed position.
    fn seed_log(path: &Path, content: &str) -> LogFilePosition {
        std::fs::write(path, content).expect("write seed");
        log_file_position(path).expect("seed position")
    }

    /// Atomically replace the file at `path` with a new file holding `content`
    /// (rename-based rotation, so the path points at a different file object).
    fn replace_log(path: &Path, content: &str) {
        let staged = path.with_extension("staged");
        std::fs::write(&staged, content).expect("write replacement");
        std::fs::rename(&staged, path).expect("rename replacement over log");
    }

    #[test]
    fn test_read_new_log_bytes_returns_only_appended_text() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("tail.log");
        let consumed = seed_log(&path, "line1\n");

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open append");
        file.write_all(b"line2\n").expect("append");
        drop(file);

        let chunk = read_new_log_bytes(&path, consumed).expect("read new bytes");
        assert_eq!(chunk.text, "line2\n");
        assert_eq!(chunk.position.byte_offset, consumed.byte_offset + 6);
        assert_eq!(chunk.position.identity, consumed.identity);
        assert!(!chunk.reset);
    }

    #[test]
    fn test_read_new_log_bytes_reports_no_change_when_unchanged() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("idle.log");
        let consumed = seed_log(&path, "content\n");

        let chunk = read_new_log_bytes(&path, consumed).expect("read new bytes");
        assert!(chunk.text.is_empty());
        assert_eq!(chunk.position, consumed);
        assert!(!chunk.reset);
    }

    #[test]
    fn test_read_new_log_bytes_resets_on_truncation() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rotated.log");
        let consumed = seed_log(&path, "old long content\n");

        // Truncate in place: same file object, now shorter than the offset.
        std::fs::write(&path, "fresh\n").expect("truncate");

        let chunk = read_new_log_bytes(&path, consumed).expect("read new bytes");
        assert_eq!(chunk.text, "fresh\n");
        assert_eq!(chunk.position.byte_offset, 6);
        assert_eq!(chunk.position.identity, consumed.identity);
        assert!(chunk.reset);
    }

    #[test]
    fn test_read_new_log_bytes_resets_on_equal_size_replacement() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rotated.log");
        let consumed = seed_log(&path, "old\n");

        replace_log(&path, "new\n");

        let chunk = read_new_log_bytes(&path, consumed).expect("read new bytes");
        assert_eq!(chunk.text, "new\n");
        assert_eq!(chunk.position.byte_offset, 4);
        assert_ne!(chunk.position.identity, consumed.identity);
        assert!(chunk.reset);
    }

    #[test]
    fn test_read_new_log_bytes_resets_on_larger_replacement() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rotated.log");
        let consumed = seed_log(&path, "old\n");

        replace_log(&path, "replacement line 1\nreplacement line 2\n");

        let chunk = read_new_log_bytes(&path, consumed).expect("read new bytes");
        assert_eq!(chunk.text, "replacement line 1\nreplacement line 2\n");
        assert_eq!(chunk.position.byte_offset, chunk.text.len() as u64);
        assert_ne!(chunk.position.identity, consumed.identity);
        assert!(chunk.reset);
    }

    #[test]
    fn test_read_new_log_bytes_appends_after_replacement_is_consumed() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rotated.log");
        let consumed = seed_log(&path, "old\n");

        replace_log(&path, "new\n");
        let rotated = read_new_log_bytes(&path, consumed).expect("read rotation");
        assert!(rotated.reset);

        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(&path)
            .expect("open append");
        file.write_all(b"more\n").expect("append");
        drop(file);

        let chunk = read_new_log_bytes(&path, rotated.position).expect("read new bytes");
        assert_eq!(chunk.text, "more\n");
        assert_eq!(chunk.position.byte_offset, 9);
        assert!(!chunk.reset);
    }
}
