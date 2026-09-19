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

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::TabId;

/// Per-tab tail bookkeeping, held centrally in `Fulgur` and keyed by tab id.
pub struct LogTailState {
    /// Byte offset in the file up to which content has already been consumed.
    pub byte_offset: u64,
    /// Identity of the file object the offset refers to, used to detect
    /// rename-based rotation even when the replacement is not shorter.
    pub identity: Option<LogFileIdentity>,
}

impl LogTailState {
    /// Create a fresh tail state seeded at the given file position.
    ///
    /// ### Arguments
    /// - `position`: The file length and identity already consumed by the seed
    ///
    /// ### Returns
    /// - `LogTailState`: The initialized state
    fn new(position: LogFilePosition) -> Self {
        Self {
            byte_offset: position.byte_offset,
            identity: position.identity,
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
    }
}

impl Fulgur {
    /// Borrow an editor tab by id.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab id to look up
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(&EditorTab)`: The matching editor tab
    /// - `None`: If no editor tab has that id
    fn editor_tab<'a>(
        &self,
        tab_id: TabId,
        cx: &'a gpui_kit::App,
    ) -> Option<&'a crate::fulgur::editor_tab::EditorTab> {
        self.tabs.iter().find_map(|tab| {
            tab.read(cx)
                .as_editor()
                .filter(|editor| editor.id == tab_id)
        })
    }
}
