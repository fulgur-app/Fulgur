//! `Fulgur` user actions and the log-view activation lifecycle.

use crate::fulgur::ui::tabs::tab::TabId;
use gpui::{AppContext, Context, Window};
use gpui_component::{WindowExt, notification::NotificationType};

use super::input::{make_log_input_state, write_log_to_bottom};
use super::tail::{log_toggle_available, trim_to_last_lines};
use super::{LOG_LINE_CAP, LogTailState};
use crate::fulgur::Fulgur;

/// File size beyond which "Load full file" warns the user about memory use.
const LOAD_FULL_WARN_BYTES: u64 = 50 * 1024 * 1024;

impl Fulgur {
    /// Toggle the active tab between the editor and the log view.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn toggle_log_view(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.get_active_editor_tab(cx) else {
            return;
        };
        let Some(path) = editor.file_path() else {
            return;
        };
        if !log_toggle_available(path) {
            return;
        }
        let tab_id = editor.id;
        if editor.log_view {
            self.deactivate_log_view(tab_id, cx);
        } else {
            self.activate_log_view(tab_id, window, cx);
        }
        cx.notify();
    }

    /// Toggle the auto-follow (scroll-to-bottom) behavior of the active log tab.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn toggle_log_follow(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.get_active_editor_tab(cx) else {
            return;
        };
        if !editor.log_view {
            return;
        }
        let tab_id = editor.id;
        let enable = !editor.log_follow;
        self.set_log_follow(tab_id, enable, cx);
        if enable {
            // Flush any text buffered while paused and snap to the bottom.
            self.flush_log_follow(tab_id, window, cx);
        }
        cx.notify();
    }

    /// Lift the line cap on the active log tab and reload the full file.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn load_full_log(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.get_active_editor_tab(cx) else {
            return;
        };
        if !editor.log_view {
            return;
        }
        let tab_id = editor.id;
        let Some(path) = editor.file_path().cloned() else {
            return;
        };
        let Some(log_content) = editor.log_content.clone() else {
            return;
        };
        let bytes = match std::fs::read(&path) {
            Ok(bytes) => bytes,
            Err(error) => {
                window.push_notification(
                    (
                        NotificationType::Error,
                        gpui::SharedString::from(format!("Failed to load full log: {error}")),
                    ),
                    cx,
                );
                return;
            }
        };
        if bytes.len() as u64 > LOAD_FULL_WARN_BYTES {
            window.push_notification(
                (
                    NotificationType::Warning,
                    gpui::SharedString::from(
                        "Loading a very large log file may use significant memory.",
                    ),
                ),
                cx,
            );
        }
        let new_offset = bytes.len() as u64;
        let full = String::from_utf8_lossy(&bytes).into_owned();
        write_log_to_bottom(&log_content, &full, window, cx);
        if let Some(state) = self.log_tail_state.get_mut(&tab_id) {
            state.byte_offset = new_offset;
            state.dropped_lines = false;
            state.pending.clear();
        }
        self.update_editor_tab(tab_id, cx, |editor, _| {
            editor.log_full = true;
            editor.log_follow = true;
        });
        cx.notify();
    }

    /// Activate log view for a tab: seed the buffer and start tailing.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to activate log view on
    /// - `window`: The active window
    /// - `cx`: The application context
    pub(crate) fn activate_log_view(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let (path, seed) = match self.editor_tab(tab_id, cx) {
            Some(editor) => match editor.file_path().cloned() {
                Some(path) => (path, editor.content.read(cx).text().to_string()),
                None => return,
            },
            None => return,
        };
        let byte_offset = std::fs::metadata(&path).map_or(0, |m| m.len());
        let (display, dropped) = trim_to_last_lines(seed, LOG_LINE_CAP);
        let soft_wrap = self.settings.editor_settings.soft_wrap;
        let log_content = cx.new(|cx| make_log_input_state(window, cx, &display, soft_wrap));
        self.update_editor_tab(tab_id, cx, |editor, _| {
            editor.log_view = true;
            editor.log_follow = true;
            editor.log_full = false;
            editor.log_content = Some(log_content);
        });
        self.log_tail_state
            .insert(tab_id, LogTailState::new(byte_offset, dropped));
        self.start_log_poll_task(tab_id, path, window, cx);
    }

    /// Fully deactivate log view for a tab, returning to the editor surface.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to deactivate log view on
    /// - `cx`: The application context
    pub(crate) fn deactivate_log_view(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        self.stop_log_poll_task(tab_id);
        self.log_tail_state.remove(&tab_id);
        self.update_editor_tab(tab_id, cx, |editor, _| {
            editor.log_view = false;
            editor.log_full = false;
            editor.log_content = None;
        });
        cx.notify();
    }

    /// Resume tailing for an already-seeded log tab, or seed it if needed.
    ///
    /// Used when switching to a tab that is in log view.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab becoming active
    /// - `window`: The active window
    /// - `cx`: The application context
    pub(crate) fn resume_log_view(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let needs_seed = self
            .editor_tab(tab_id, cx)
            .is_some_and(|editor| editor.log_content.is_none());
        let path = self
            .editor_tab(tab_id, cx)
            .and_then(|e| e.file_path().cloned());
        if needs_seed {
            self.activate_log_view(tab_id, window, cx);
        } else if let Some(path) = path {
            self.start_log_poll_task(tab_id, path, window, cx);
        }
    }

    /// Drop all tail bookkeeping for a removed tab.
    ///
    /// ### Arguments
    /// - `tab_id`: The removed tab id
    pub(crate) fn clear_log_tail(&mut self, tab_id: TabId) {
        self.stop_log_poll_task(tab_id);
        self.log_tail_state.remove(&tab_id);
    }
}
