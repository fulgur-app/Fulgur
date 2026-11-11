//! `Fulgur` user actions and the log-view activation lifecycle.

use crate::fulgur::ui::tabs::tab::TabId;
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{Context, SharedString, Window};

use super::LogTailState;
use super::polling::snap_to_last_line;
use super::tail::{log_file_position, log_toggle_available, tail_encoding};
use crate::fulgur::Fulgur;
use crate::fulgur::utils::logger;

impl Fulgur {
    /// Open Fulgur's own current log file, which opens in log view as any `.log` file.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn open_application_log(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        match logger::current_log_file() {
            Some(path) => self.do_open_file(window, cx, path),
            None => window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from("The application log file could not be found."),
                ),
                cx,
            ),
        }
    }

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
        } else if tail_encoding(&editor.encoding).is_none() {
            let notice = format!("Log view is not available for {} files.", editor.encoding);
            window.push_notification((NotificationType::Warning, SharedString::from(notice)), cx);
        } else if editor.modified {
            window.push_notification(
                (
                    NotificationType::Warning,
                    SharedString::from(
                        "Save or discard your changes before switching to log view.",
                    ),
                ),
                cx,
            );
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
        let content = editor.content.clone();
        self.update_editor_tab(tab_id, cx, |editor, _| {
            editor.log_follow = enable;
        });
        if enable {
            snap_to_last_line(&content, window, cx);
        }
        cx.notify();
    }

    /// Activate log view for a tab: make its buffer read-only and start tailing.
    ///
    /// The buffer normally already mirrors the file as loaded by the editor, so
    /// only the consumed position is seeded from the file. When the two byte
    /// lengths disagree (the file grew while the tab was inactive, or it is not
    /// UTF-8) the poll task first rereads the whole file in the background,
    /// decoded with the tab's encoding, and replaces the buffer, so the buffer
    /// and the offset agree.
    ///
    /// A tab whose encoding cannot be tailed (see `tail_encoding`) stays in
    /// the editor: its `log_view` flag, set by a restore or a transfer, is
    /// cleared instead.
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
        let Some((path, content, encoding_label)) =
            self.editor_tab(tab_id, cx).and_then(|editor| {
                Some((
                    editor.file_path().cloned()?,
                    editor.content.clone(),
                    editor.encoding.clone(),
                ))
            })
        else {
            return;
        };
        let Some(encoding) = tail_encoding(&encoding_label) else {
            log::info!(
                "Log view unavailable for {} ({encoding_label}), keeping the editor",
                path.display()
            );
            self.update_editor_tab(tab_id, cx, |editor, _| {
                editor.log_view = false;
            });
            return;
        };
        let buffer_len = content.read(cx).text().len() as u64;
        let tail_state = match log_file_position(&path) {
            Some(position) if position.byte_offset == buffer_len => {
                LogTailState::new(position, encoding)
            }
            _ => LogTailState::awaiting_reseed(encoding),
        };
        let highlight_colors = self.settings.editor_settings.highlight_colors;
        self.update_editor_tab(tab_id, cx, |editor, cx| {
            editor.log_view = true;
            editor.log_follow = true;
            // The rendered `Editor` re-applies the flag every frame; setting it
            // here too keeps `is_editable` right until that first frame.
            editor.content.update(cx, |state, cx| {
                state.set_readonly(true, cx);
            });
            editor.set_highlight_colors(cx, highlight_colors);
            editor.mark_as_saved(cx);
        });
        snap_to_last_line(&content, window, cx);
        self.log_tail_state.insert(tab_id, tail_state);
        self.start_log_poll_task(tab_id, path, window, cx);
    }

    /// Fully deactivate log view for a tab, returning to the editable surface.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to deactivate log view on
    /// - `cx`: The application context
    pub(crate) fn deactivate_log_view(&mut self, tab_id: TabId, cx: &mut Context<Self>) {
        self.stop_log_poll_task(tab_id);
        self.log_tail_state.remove(&tab_id);
        let highlight_colors = self.settings.editor_settings.highlight_colors;
        self.update_editor_tab(tab_id, cx, |editor, cx| {
            editor.log_view = false;
            let stays_locked = editor.blocks_user_edits();
            editor.content.update(cx, |state, cx| {
                state.set_readonly(stays_locked, cx);
            });
            editor.set_highlight_colors(cx, highlight_colors);
            editor.mark_as_saved(cx);
        });
        cx.notify();
    }

    /// Resume tailing for a tab that is in log view, or activate it if needed.
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
        let path = self
            .editor_tab(tab_id, cx)
            .and_then(|editor| editor.file_path().cloned());
        if !self.log_tail_state.contains_key(&tab_id) {
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
