use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::TabId;
use gpui::{Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::PathBuf;

/// Snapshot of a tab's saved-content baseline, captured before an optimistic
/// save so a failed background write can restore it.
pub(super) struct SavedBaseline {
    /// The tab's `original_content_hash` at dispatch time
    pub(super) hash: u64,
    /// The tab's `original_content_len` at dispatch time
    pub(super) len: usize,
    /// The tab's `modified` flag at dispatch time
    pub(super) modified: bool,
}

/// Dispatch-time context of a background save, handed back to the completion
/// handler that runs on the UI thread once the write resolves.
pub(super) struct SaveCompletion {
    /// Stable identifier of the editor tab being saved
    pub(super) tab_id: TabId,
    /// Destination path of the write, as requested at dispatch
    pub(super) path: PathBuf,
    /// Size of the written content in bytes
    pub(super) byte_len: usize,
    /// Saved baseline captured at dispatch, restored if the write fails
    pub(super) previous_baseline: Option<SavedBaseline>,
}

impl Fulgur {
    /// Capture a tab's saved-content baseline before an optimistic save.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(SavedBaseline)`: The tab's current baseline and modified flag
    /// - `None`: If the tab no longer exists or is not an editor tab
    pub(super) fn capture_saved_baseline(
        &self,
        tab_id: TabId,
        cx: &Context<Self>,
    ) -> Option<SavedBaseline> {
        self.tab_entity_of(tab_id, cx).and_then(|tab| {
            tab.read(cx).as_editor().map(|editor_tab| SavedBaseline {
                hash: editor_tab.original_content_hash,
                len: editor_tab.original_content_len,
                modified: editor_tab.modified,
            })
        })
    }

    /// Report a failed background save and roll back the optimistic saved state.
    ///
    /// ### Arguments
    /// - `completion`: Dispatch-time context of the save that failed
    /// - `error`: The write error to report
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn handle_failed_save(
        &mut self,
        completion: SaveCompletion,
        error: &anyhow::Error,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::error!("Failed to save file {}: {error}", completion.path.display());
        if let Some(baseline) = completion.previous_baseline {
            self.update_editor_tab(completion.tab_id, cx, |editor_tab, cx| {
                let edited_during_save = editor_tab.modified;
                editor_tab.original_content_hash = baseline.hash;
                editor_tab.original_content_len = baseline.len;
                if editor_tab.large_file {
                    editor_tab.modified = baseline.modified || edited_during_save;
                } else {
                    editor_tab.check_modified(cx);
                }
                cx.notify();
            });
        }
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
