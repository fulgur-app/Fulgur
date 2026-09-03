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

use gpui::{Context, Entity, Window};
use gpui_component::input::EditorState;

use self::input::{append_log_to_bottom, write_log_to_bottom};
use self::tail::trim_to_last_lines;
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

/// How a log display buffer should absorb a piece of newly produced text.
#[derive(Clone, Copy)]
enum LogDisplayUpdate<'a> {
    Replace(&'a str),
    Append(&'a str),
}

impl Fulgur {
    /// Write text into a log display buffer and update its tail bookkeeping.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab whose tail state should be updated
    /// - `log_content`: The read-only display buffer of that tab
    /// - `update`: The text to write and how to write it
    /// - `log_full`: Whether the line cap is lifted for this tab
    /// - `window`: The active window
    /// - `cx`: The application context
    fn commit_log_display(
        &mut self,
        tab_id: TabId,
        log_content: &Entity<EditorState>,
        update: LogDisplayUpdate<'_>,
        log_full: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let dropped = match update {
            LogDisplayUpdate::Replace(text) if log_full => {
                write_log_to_bottom(log_content, text, window, cx);
                false
            }
            LogDisplayUpdate::Replace(text) => {
                let (display, dropped) = trim_to_last_lines(text.to_string(), LOG_LINE_CAP);
                write_log_to_bottom(log_content, &display, window, cx);
                dropped
            }
            LogDisplayUpdate::Append(text) => {
                let dropped_before = self
                    .log_tail_state
                    .get(&tab_id)
                    .is_some_and(|state| state.dropped_lines);
                let dropped_now = append_log_to_bottom(log_content, text, log_full, window, cx);
                dropped_before || dropped_now
            }
        };
        if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
            state.pending.clear();
            state.dropped_lines = dropped;
        }
    }
}
