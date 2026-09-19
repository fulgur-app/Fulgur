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

/// Return the length of an incomplete UTF-8 sequence at the end of a buffer.
///
/// ### Arguments
/// - `bytes`: The raw bytes read from the file
///
/// ### Returns
/// - `usize`: The number of trailing bytes (at most 3) that start a character
///   whose remaining bytes have not been read yet, or `0`
fn incomplete_utf8_suffix_len(bytes: &[u8]) -> usize {
    let Some(last) = bytes.utf8_chunks().last() else {
        return 0;
    };
    let tail = last.invalid();
    match std::str::from_utf8(tail) {
        Err(error) if error.error_len().is_none() => tail.len(),
        _ => 0,
    }
}

/// Decode log bytes as UTF-8, holding back an incomplete trailing character.
///
/// ### Arguments
/// - `bytes`: The raw bytes read from the file
///
/// ### Returns
/// - `(String, u64)`: The decoded text and the number of bytes it consumed
fn decode_log_bytes(bytes: &[u8]) -> (String, u64) {
    let consumed = bytes.len() - incomplete_utf8_suffix_len(bytes);
    (
        String::from_utf8_lossy(&bytes[..consumed]).into_owned(),
        consumed as u64,
    )
}

/// Read the rest of an opened file from its current position as a chunk.
///
/// ### Arguments
/// - `file`: The opened file, positioned where reading should start
/// - `start_offset`: The file offset that position corresponds to
/// - `identity`: The identity of the file object
/// - `reset`: Whether the chunk replaces the display rather than extending it
///
/// ### Returns
/// - `Ok(LogTailChunk)`: The decoded text and the consumed position
/// - `Err(std::io::Error)`: If the file could not be read
fn read_chunk_to_end(
    file: &mut File,
    start_offset: u64,
    identity: Option<LogFileIdentity>,
    reset: bool,
) -> std::io::Result<LogTailChunk> {
    let mut bytes = Vec::new();
    file.read_to_end(&mut bytes)?;
    let (text, consumed) = decode_log_bytes(&bytes);
    Ok(LogTailChunk {
        text,
        position: LogFilePosition {
            byte_offset: start_offset + consumed,
            identity,
        },
        reset,
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
        return read_chunk_to_end(&mut file, 0, identity, true).ok();
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
    read_chunk_to_end(&mut file, offset, identity, false).ok()
}

#[cfg(test)]
mod tests {
    use super::{
        LogFilePosition, decode_log_bytes, log_file_position, log_toggle_available,
        opens_as_log_by_default, read_new_log_bytes,
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

    /// Append raw bytes to the file at `path`.
    fn append_log(path: &Path, bytes: &[u8]) {
        let mut file = std::fs::OpenOptions::new()
            .append(true)
            .open(path)
            .expect("open append");
        file.write_all(bytes).expect("append");
    }

    #[test]
    fn test_decode_holds_back_every_split_of_multibyte_characters() {
        for character in ["\u{e9}", "\u{20ac}", "\u{1f600}"] {
            let bytes = character.as_bytes();
            let text = format!("before {character} after\n");
            let text_bytes = text.as_bytes();
            let char_start = text.find(character).expect("character present");
            for split in 1..bytes.len() {
                let (head, consumed) = decode_log_bytes(&text_bytes[..char_start + split]);
                assert_eq!(head, "before ", "split {split} of {character}");
                assert_eq!(consumed, char_start as u64, "split {split} of {character}");
                let (rest, rest_consumed) = decode_log_bytes(&text_bytes[char_start..]);
                assert_eq!(rest, format!("{character} after\n"));
                assert_eq!(rest_consumed, (text_bytes.len() - char_start) as u64);
            }
        }
    }

    #[test]
    fn test_decode_consumes_complete_text_fully() {
        let text = "caf\u{e9} \u{20ac} \u{1f600}\n";
        let (decoded, consumed) = decode_log_bytes(text.as_bytes());
        assert_eq!(decoded, text);
        assert_eq!(consumed, text.len() as u64);
        assert_eq!(decode_log_bytes(b""), (String::new(), 0));
    }

    #[test]
    fn test_decode_replaces_invalid_bytes_without_holding_them() {
        let (decoded, consumed) = decode_log_bytes(b"ok\xff");
        assert_eq!(decoded, "ok\u{fffd}");
        assert_eq!(consumed, 3);

        let (decoded, consumed) = decode_log_bytes(b"\xe2Ab");
        assert_eq!(decoded, "\u{fffd}Ab");
        assert_eq!(consumed, 3);

        // A truncated sequence that is already ill-formed (lone surrogate) is
        // replaced immediately rather than waiting for more bytes.
        let (decoded, consumed) = decode_log_bytes(b"x\xed\xa0");
        assert_eq!(decoded, "x\u{fffd}\u{fffd}");
        assert_eq!(consumed, 3);
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
    fn test_read_new_log_bytes_completes_character_split_across_polls() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("split.log");
        let consumed = seed_log(&path, "line1\n");

        append_log(&path, b"\xe2");
        let first = read_new_log_bytes(&path, consumed).expect("read first half");
        assert!(first.text.is_empty());
        assert_eq!(first.position.byte_offset, consumed.byte_offset);
        assert!(!first.reset);

        // Nothing new: the held-back byte is re-read but still not emitted.
        let idle = read_new_log_bytes(&path, first.position).expect("read idle");
        assert!(idle.text.is_empty());
        assert_eq!(idle.position, first.position);

        append_log(&path, b"\x82\xac\n");
        let second = read_new_log_bytes(&path, idle.position).expect("read second half");
        assert_eq!(second.text, "\u{20ac}\n");
        assert_eq!(second.position.byte_offset, consumed.byte_offset + 4);
        assert!(!second.reset);
    }

    #[test]
    fn test_read_new_log_bytes_holds_back_incomplete_suffix_on_reset() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("rotated.log");
        let consumed = seed_log(&path, "old long content\n");

        std::fs::write(&path, b"new\xf0\x9f").expect("truncate");
        let reset = read_new_log_bytes(&path, consumed).expect("read reset");
        assert_eq!(reset.text, "new");
        assert_eq!(reset.position.byte_offset, 3);
        assert!(reset.reset);

        append_log(&path, b"\x98\x80\n");
        let chunk = read_new_log_bytes(&path, reset.position).expect("read completion");
        assert_eq!(chunk.text, "\u{1f600}\n");
        assert_eq!(chunk.position.byte_offset, 8);
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
