use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{Fulgur, PendingSaveCloseAction};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{Context, SharedString, Window};
use std::path::PathBuf;

/// Dispatch-time context of a background save, handed back to the completion
/// handler that runs on the UI thread once the write resolves.
pub(super) struct SaveCompletion {
    /// Stable identifier of the editor tab being saved
    pub(super) tab_id: TabId,
    /// Destination path of the write, as requested at dispatch
    pub(super) path: PathBuf,
    /// Size of the written content in bytes
    pub(super) byte_len: usize,
    /// Fingerprint of the exact editor snapshot dispatched to the writer
    pub(super) saved_content_hash: u64,
    /// UTF-8 byte length of the exact editor snapshot dispatched to the writer
    pub(super) saved_content_len: usize,
}

impl Fulgur {
    /// Defer a tab close until its background local save resolves.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the tab requested for close
    ///
    /// ### Returns
    /// - `true`: A save is pending and the close was queued
    /// - `false`: The tab has no pending local save
    pub(crate) fn defer_tab_close_for_pending_save(&mut self, tab_id: TabId) -> bool {
        if !self.inflight_saves.contains_key(&tab_id) {
            return false;
        }
        self.pending_save_tab_closes.insert(tab_id);
        true
    }

    /// Defer an application or window close until every local save in this
    /// window resolves.
    ///
    /// ### Arguments
    /// - `action`: Close operation to resume after all pending saves succeed
    ///
    /// ### Returns
    /// - `true`: At least one save is pending and the close was queued
    /// - `false`: There are no pending local saves
    pub(crate) fn defer_close_for_pending_saves(&mut self, action: PendingSaveCloseAction) -> bool {
        if self.inflight_saves.is_empty() {
            return false;
        }
        self.pending_save_tab_closes.clear();
        self.pending_save_close_action = Some(action);
        true
    }

    /// Continue close work that was waiting for a successful local save.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the tab whose save completed
    /// - `window`: Window containing the completed save
    /// - `cx`: Application context for resuming the deferred close
    pub(super) fn resume_after_local_save(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inflight_saves.is_empty()
            && let Some(action) = self.pending_save_close_action.take()
        {
            self.pending_save_tab_closes.clear();
            match action {
                PendingSaveCloseAction::Quit => self.quit(window, cx),
                PendingSaveCloseAction::Window => {
                    if self.on_window_close_requested(window, cx) {
                        window.remove_window();
                    }
                }
            }
            return;
        }

        if self.pending_save_tab_closes.remove(&tab_id) {
            self.close_tab(tab_id, window, cx);
        }
    }

    /// Cancel close work whose safety depended on a local save that failed.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the tab whose save failed
    pub(super) fn cancel_close_after_failed_local_save(&mut self, tab_id: TabId) {
        self.pending_save_tab_closes.remove(&tab_id);
        self.pending_save_close_action = None;
    }

    /// Report a failed background save while preserving the live dirty buffer.
    ///
    /// ### Arguments
    /// - `completion`: Dispatch-time context of the save that failed
    /// - `error`: The write error to report
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn handle_failed_save(
        completion: &SaveCompletion,
        error: &anyhow::Error,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::error!("Failed to save file {}: {error}", completion.path.display());
        let file_name = completion
            .path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file");
        window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!("Failed to save '{file_name}': {error}")),
            ),
            cx,
        );
        cx.notify();
    }
}
