//! Log view mode: a live "tail -f" surface for log-like files.
//!
//! A log-view tab is the regular editor tab with its `content` buffer put in
//! read-only mode and kept in sync with the file by a per-active-tab polling
//! task. The poll reads only newly appended bytes and appends them to the
//! buffer with programmatic edits, which bypass the read-only guard while the
//! user can still select, copy and search the text.
//!
//! Following is an explicit, user-controlled toggle (`log_follow`): when on,
//! every refresh snaps the caret to the last line; when off, new text is still
//! appended but the caret and viewport stay where the user left them. The
//! mode toggle and the `Follow` control live in the status bar; this module
//! owns the state machine and tailing logic only (it renders no UI of its own).

mod lifecycle;
mod polling;
mod tail;
#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

pub use tail::{LogFileIdentity, LogFilePosition, log_toggle_available, opens_as_log_by_default};

use encoding_rs::Encoding;
use std::path::Path;
use tail::{LogTailChunk, read_new_log_bytes, read_whole_log};

/// Per-tab tail bookkeeping, held centrally in `Fulgur` and keyed by tab id.
#[derive(Clone, Copy)]
pub struct LogTailState {
    /// Byte offset in the file up to which content has already been consumed.
    pub byte_offset: u64,
    /// Identity of the file object the offset refers to, used to detect
    /// rename-based rotation even when the replacement is not shorter.
    pub identity: Option<LogFileIdentity>,
    /// Whether the buffer lags the file and must be replaced by a full reread
    /// before tailing can resume from `byte_offset`.
    pub reseed_pending: bool,
    /// The tab's encoding, used to decode every read so the buffer matches
    /// what the editor loaded and what a save writes back.
    pub encoding: &'static Encoding,
}

impl LogTailState {
    /// Create a fresh tail state seeded at the given file position.
    ///
    /// ### Arguments
    /// - `position`: The file length and identity already consumed by the seed
    /// - `encoding`: The encoding to decode appended bytes with
    ///
    /// ### Returns
    /// - `LogTailState`: The initialized state
    fn new(position: LogFilePosition, encoding: &'static Encoding) -> Self {
        Self {
            byte_offset: position.byte_offset,
            identity: position.identity,
            reseed_pending: false,
            encoding,
        }
    }

    /// Create a tail state whose first read replaces the whole buffer.
    ///
    /// ### Arguments
    /// - `encoding`: The encoding to decode the file with
    ///
    /// ### Returns
    /// - `LogTailState`: The state awaiting a full reread
    fn awaiting_reseed(encoding: &'static Encoding) -> Self {
        Self {
            byte_offset: 0,
            identity: None,
            reseed_pending: true,
            encoding,
        }
    }

    /// Read the next chunk of the tailed file: the whole file while a reseed
    /// is pending, otherwise only the bytes appended past the consumed position.
    ///
    /// ### Arguments
    /// - `path`: The file to read
    ///
    /// ### Returns
    /// - `Some(LogTailChunk)`: The text read and the position it advanced to
    /// - `None`: If the file could not be opened, stat-ed, or read
    fn read_next_chunk(&self, path: &Path) -> Option<LogTailChunk> {
        if self.reseed_pending {
            read_whole_log(path, self.encoding)
        } else {
            read_new_log_bytes(path, self.position(), self.encoding)
        }
    }

    /// Return the consumed position as a single value for the tail reader.
    ///
    /// ### Returns
    /// - `LogFilePosition`: The consumed byte offset and file identity
    fn position(&self) -> LogFilePosition {
        LogFilePosition {
            byte_offset: self.byte_offset,
            identity: self.identity,
        }
    }

    /// Advance the consumed position after a read.
    ///
    /// ### Arguments
    /// - `position`: The new consumed byte offset and file identity
    fn set_position(&mut self, position: LogFilePosition) {
        self.byte_offset = position.byte_offset;
        self.identity = position.identity;
        self.reseed_pending = false;
    }
}
