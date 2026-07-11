//! Log view mode: a live "tail -f" surface for log-like files.
//!
//! A log-view tab keeps its editable `content` buffer untouched and instead
//! renders a dedicated read-only `log_content` buffer that is refreshed by a
//! per-active-tab polling task. The poll reads only newly appended bytes,
//! appends them to the display, and trims to the last `LOG_LINE_CAP` lines
//! (unless the user loaded the full file).
//!
//! Following is an explicit, user-controlled toggle (`log_follow`): when on,
//! every refresh snaps to the bottom; when off, the view is frozen and new
//! text is buffered until the user re-enables follow. The mode toggle and the
//! `Follow` / `Load full` controls live in the status bar; this module owns the
//! state machine and tailing logic only (it renders no UI of its own).

mod input;
mod lifecycle;
mod polling;
mod tail;

pub use tail::{log_toggle_available, opens_as_log_by_default};

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::TabId;

/// Maximum number of trailing lines kept in the log view before trimming.
pub const LOG_LINE_CAP: usize = 10_000;

/// Per-tab tail bookkeeping, held centrally in `Fulgur` and keyed by tab id.
pub struct LogTailState {
    /// Byte offset in the file up to which content has already been consumed.
    pub byte_offset: u64,
    /// Whether the line cap has dropped older lines from the display.
    pub dropped_lines: bool,
    /// Newly appended text accumulated while follow is paused (frozen view).
    pub pending: String,
}

impl LogTailState {
    /// Create a fresh tail state seeded at the given byte offset.
    ///
    /// ### Arguments
    /// - `byte_offset`: The initial file offset already consumed by the seed
    /// - `dropped_lines`: Whether the seed already exceeded the line cap
    ///
    /// ### Returns
    /// - `LogTailState`: The initialized state
    fn new(byte_offset: u64, dropped_lines: bool) -> Self {
        Self {
            byte_offset,
            dropped_lines,
            pending: String::new(),
        }
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
        cx: &'a gpui::App,
    ) -> Option<&'a crate::fulgur::editor_tab::EditorTab> {
        self.tabs.iter().find_map(|tab| {
            tab.read(cx)
                .as_editor()
                .filter(|editor| editor.id == tab_id)
        })
    }
}
