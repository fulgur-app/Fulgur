//! The `Fulgur` tail engine: the background poll task and chunk application.

use crate::fulgur::ui::tabs::tab::TabId;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use gpui::{Context, Window};

use super::LOG_LINE_CAP;
use super::input::write_log_to_bottom;
use super::tail::{read_new_log_bytes, trim_to_last_lines};
use crate::fulgur::Fulgur;

/// How often the active log tab polls its file for newly appended bytes.
const POLL_INTERVAL_MS: u64 = 250;

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
                let Ok(Ok(Some(offset))) =
                    window.update(|_, cx| view.update(cx, |this, _| this.log_tail_offset(tab_id)))
                else {
                    break;
                };
                let read_path = path.clone();
                let chunk = window
                    .background_executor()
                    .spawn(async move { read_new_log_bytes(&read_path, offset) })
                    .await;
                let Some((text, new_offset, truncated)) = chunk else {
                    continue;
                };
                if text.is_empty() && !truncated {
                    continue;
                }
                let applied = window.update(|window, cx| {
                    view.update(cx, |this, cx| {
                        this.apply_log_tail_chunk(tab_id, &text, new_offset, truncated, window, cx);
                    })
                });
                if applied.is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Return the current consumed byte offset for a tailing tab.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to query
    ///
    /// ### Returns
    /// - `Some(u64)`: The byte offset when the tab is still in log view
    /// - `None`: When the tab is gone or no longer in log view (poll should stop)
    fn log_tail_offset(&self, tab_id: TabId) -> Option<u64> {
        let editor = self.editor_tab(tab_id)?;
        if !editor.log_view {
            return None;
        }
        self.log_tail_state
            .get(&tab_id)
            .map(|state| state.byte_offset)
    }

    /// Apply a freshly read chunk of log bytes to the display buffer.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab being tailed
    /// - `text`: The newly read text
    /// - `new_offset`: The new consumed byte offset
    /// - `truncated`: Whether the file was truncated/rotated
    /// - `window`: The active window
    /// - `cx`: The application context
    fn apply_log_tail_chunk(
        &mut self,
        tab_id: TabId,
        text: &str,
        new_offset: u64,
        truncated: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (log_content, log_full, follow) = match self.editor_tab(tab_id) {
            Some(editor) => match editor.log_content.clone() {
                Some(log_content) => (log_content, editor.log_full, editor.log_follow),
                None => return,
            },
            None => return,
        };

        // Advance the consumed offset regardless of follow state so paused tabs
        // resume from the right place.
        if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
            state.byte_offset = new_offset;
        }

        if truncated {
            // File was rotated or shrunk: rebuild the view from the new content.
            let (display, dropped) = if log_full {
                (text.to_string(), false)
            } else {
                trim_to_last_lines(text.to_string(), LOG_LINE_CAP)
            };
            if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
                state.pending.clear();
                state.dropped_lines = dropped;
            }
            write_log_to_bottom(&log_content, &display, window, cx);
            cx.notify();
            return;
        }

        if !follow {
            // Follow is paused: freeze the view and buffer the new text until the
            // user re-enables follow, which flushes the buffer to the bottom.
            if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
                state.pending.push_str(text);
            }
            return;
        }

        // Following: append the new text to the live buffer and snap to bottom.
        let dropped_before = self
            .log_tail_state
            .get(&tab_id)
            .is_some_and(|state| state.dropped_lines);
        let mut combined = log_content.read(cx).text().to_string();
        combined.push_str(text);
        let (display, dropped_now) = if log_full {
            (combined, false)
        } else {
            trim_to_last_lines(combined, LOG_LINE_CAP)
        };
        write_log_to_bottom(&log_content, &display, window, cx);
        if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
            state.pending.clear();
            state.dropped_lines = dropped_before || dropped_now;
        }
        cx.notify();
    }

    /// Flush text buffered while follow was paused and snap to the bottom.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to flush
    /// - `window`: The active window
    /// - `cx`: The application context
    pub(super) fn flush_log_follow(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (log_content, log_full) = match self.editor_tab(tab_id) {
            Some(editor) => match editor.log_content.clone() {
                Some(log_content) => (log_content, editor.log_full),
                None => return,
            },
            None => return,
        };
        let pending = self
            .log_tail_state
            .get(&tab_id)
            .map(|state| state.pending.clone())
            .unwrap_or_default();
        let mut combined = log_content.read(cx).text().to_string();
        combined.push_str(&pending);
        let dropped_before = self
            .log_tail_state
            .get(&tab_id)
            .is_some_and(|state| state.dropped_lines);
        let (display, dropped_now) = if log_full {
            (combined, false)
        } else {
            trim_to_last_lines(combined, LOG_LINE_CAP)
        };
        write_log_to_bottom(&log_content, &display, window, cx);
        if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
            state.pending.clear();
            state.dropped_lines = dropped_before || dropped_now;
        }
    }

    /// Set the follow flag on a log tab by id.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to update
    /// - `follow`: The new follow state
    pub(super) fn set_log_follow(&mut self, tab_id: TabId, follow: bool) {
        if let Some(editor) = self.editor_tab_mut(tab_id) {
            editor.log_follow = follow;
        }
    }
}
