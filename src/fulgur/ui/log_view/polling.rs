//! The `Fulgur` tail engine: the background poll task and chunk application.

use crate::fulgur::ui::tabs::tab::TabId;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui_kit::component::input::{EditorState, RopeExt};
use gpui_kit::{Context, Entity, Window, point, px};

use super::tail::{LogTailChunk, read_new_log_bytes};
use super::{LogFilePosition, LogTailState};
use crate::fulgur::Fulgur;

/// How often the active log tab polls its file for newly appended bytes.
const POLL_INTERVAL_MS: u64 = 250;

/// Move the caret to the end of a buffer.
///
/// Only the selection moves: the buffer is not focused, so a search bar or a
/// dialog keeps the keyboard while the log keeps following.
///
/// ### Arguments
/// - `state`: The buffer to move the caret in
/// - `cx`: The buffer context
fn place_caret_at_end(state: &mut EditorState, cx: &mut Context<EditorState>) {
    let end = state.text().len();
    state.set_selected_range(end..end, cx);
}

/// Scroll a buffer to its bottom from the row count of its current text.
///
/// The target is exact when lines are not soft wrapped; wrapped rows make it
/// undershoot, which the layout-based snap corrects a frame later.
///
/// ### Arguments
/// - `state`: The buffer to scroll
/// - `cx`: The buffer context
fn scroll_to_estimated_bottom(state: &mut EditorState, cx: &mut Context<EditorState>) {
    let Some(line_height) = state.line_height() else {
        return;
    };
    // Row counts beyond f32's 24-bit mantissa are not realistic for a log,
    // and the target is clamped by the layout anyway.
    #[allow(clippy::cast_precision_loss)]
    let rows = state.text().lines_len() as f32;
    let content_height = line_height * rows;
    let bottom = (state.input_bounds().size.height - content_height).min(px(0.));
    let x = state.scroll_offset().x;
    state.set_scroll_offset(point(x, bottom), cx);
}

/// Pin the caret to the end of a buffer and reveal its last line.
///
/// A caret move scrolls from the last painted layout, clamped to its extent,
/// so right after a write it can only reach the previous end of the buffer.
/// The estimated bottom is applied immediately instead, and the caret is
/// moved again once a layout of the new text exists: `on_next_frame` runs
/// just before a draw, hence two hops, the first firing before the frame that
/// lays out the new text and the second right after it.
///
/// ### Arguments
/// - `content`: The buffer to snap
/// - `window`: The active window
/// - `cx`: The application context
pub(super) fn snap_to_last_line(
    content: &Entity<EditorState>,
    window: &mut Window,
    cx: &mut Context<Fulgur>,
) {
    content.update(cx, |state, cx| {
        place_caret_at_end(state, cx);
        scroll_to_estimated_bottom(state, cx);
    });
    let content = content.clone();
    window.on_next_frame(move |window, _| {
        window.on_next_frame(move |_, cx| {
            content.update(cx, place_caret_at_end);
        });
    });
}

/// Append text to the end of a log buffer with a programmatic edit.
///
/// When following, the view is snapped to the new end. Otherwise the
/// selection and scroll offset captured before the edit are restored
/// afterwards, so the user keeps reading where they were.
///
/// ### Arguments
/// - `content`: The buffer to append to
/// - `text`: The newly appended text
/// - `follow`: Whether the view should reveal the appended text
/// - `window`: The active window
/// - `cx`: The application context
pub(super) fn append_to_log_buffer(
    content: &Entity<EditorState>,
    text: &str,
    follow: bool,
    window: &mut Window,
    cx: &mut Context<Fulgur>,
) {
    content.update(cx, |state, cx| {
        let selection = state.selected_range();
        let scroll_offset = state.scroll_offset();
        place_caret_at_end(state, cx);
        state.insert(text, window, cx);
        if !follow {
            state.set_selected_range(selection, cx);
            state.set_scroll_offset(scroll_offset, cx);
        }
    });
    if follow {
        snap_to_last_line(content, window, cx);
    }
}

impl Fulgur {
    /// Stop the poll task for a tab without otherwise changing its log state.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab whose poll task should stop
    pub(crate) fn stop_log_poll_task(&mut self, tab_id: TabId) {
        if let Some(flag) = self.log_tail_cancel.remove(&tab_id) {
            flag.store(true, Ordering::Release);
        }
    }

    /// Start the per-tab poll task if one is not already running.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to tail
    /// - `path`: The file path to read
    /// - `window`: The active window
    /// - `cx`: The application context
    pub(super) fn start_log_poll_task(
        &mut self,
        tab_id: TabId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.log_tail_cancel.contains_key(&tab_id) {
            return;
        }
        let cancel = Arc::new(AtomicBool::new(false));
        self.log_tail_cancel.insert(tab_id, cancel.clone());
        cx.spawn_in(window, async move |view, window| {
            loop {
                window
                    .background_executor()
                    .timer(Duration::from_millis(POLL_INTERVAL_MS))
                    .await;
                if cancel.load(Ordering::Acquire) {
                    break;
                }
                let Ok(Ok(Some(consumed))) = window
                    .update(|_, cx| view.update(cx, |this, cx| this.log_tail_position(tab_id, cx)))
                else {
                    break;
                };
                let read_path = path.clone();
                let chunk = window
                    .background_executor()
                    .spawn(async move { read_new_log_bytes(&read_path, consumed) })
                    .await;
                let Some(chunk) = chunk else {
                    continue;
                };
                if chunk.text.is_empty() && !chunk.reset {
                    continue;
                }
                let applied = window.update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.apply_log_tail_chunk(tab_id, &chunk, window, cx);
                    })
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Return the current consumed position for a tailing tab.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to query
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(LogFilePosition)`: The consumed offset and file identity when
    ///   the tab is still in log view
    /// - `None`: When the tab is gone or no longer in log view (poll should stop)
    fn log_tail_position(&self, tab_id: TabId, cx: &gpui_kit::App) -> Option<LogFilePosition> {
        let editor = self.editor_tab(tab_id, cx)?;
        if !editor.log_view {
            return None;
        }
        self.log_tail_state.get(&tab_id).map(LogTailState::position)
    }

    /// Apply a freshly read chunk of log bytes to the tab's buffer.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab being tailed
    /// - `chunk`: The newly read text, the position it advanced to, and
    ///   whether the file was truncated or replaced
    /// - `window`: The active window
    /// - `cx`: The application context
    pub(super) fn apply_log_tail_chunk(
        &mut self,
        tab_id: TabId,
        chunk: &LogTailChunk,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((content, follow)) = self
            .editor_tab(tab_id, cx)
            .filter(|editor| editor.log_view)
            .map(|editor| (editor.content.clone(), editor.log_follow))
        else {
            return;
        };

        if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
            state.set_position(chunk.position);
        }

        if chunk.reset {
            // File was replaced or shrunk: rebuild the buffer from the new content.
            content.update(cx, |state, cx| {
                state.set_value(chunk.text.as_str(), window, cx);
            });
            snap_to_last_line(&content, window, cx);
        } else {
            append_to_log_buffer(&content, &chunk.text, follow, window, cx);
        }

        self.update_editor_tab(tab_id, cx, |editor, cx| {
            editor.mark_as_saved(cx);
        });
        cx.notify();
    }
}
