use super::super::{DecodedContents, binary_file_notice};
use super::open_core::{FileReadOutcome, read_and_classify_file};
use super::{notice_file_name, read_failure_notice};
use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    tab::Tab,
    ui::tabs::{editor_tab::initial_csv_state, tab::TabId},
};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{Context, Entity, SharedString, Window};
use std::fs::Metadata;
use std::path::{Path, PathBuf};

impl Fulgur {
    /// Start reading the file of a local placeholder tab on the background executor.
    ///
    /// Does nothing when a read for this tab is already running. The tab is
    /// resolved again by id once the read completes, so it may have been closed
    /// or moved to another window in the meantime.
    ///
    /// ### Arguments
    /// - `tab_id`: The placeholder tab waiting for the file
    /// - `path`: The local file to read
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    pub(crate) fn load_placeholder_file_content(
        &mut self,
        tab_id: TabId,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.inflight_content_loads.insert(tab_id) {
            return;
        }
        cx.spawn_in(window, async move |view, window| {
            let read_path = path.clone();
            let (outcome, disk_metadata) = window
                .background_executor()
                .spawn(async move { read_and_classify_file(&read_path) })
                .await;
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        this.apply_placeholder_file_read(
                            tab_id,
                            &path,
                            outcome,
                            disk_metadata.as_ref(),
                            window,
                            cx,
                        );
                    });
                })
                .ok();
        })
        .detach();
    }

    /// Fill a placeholder tab with the file read for it, or drop the tab when
    /// the file cannot be shown.
    ///
    /// ### Arguments
    /// - `tab_id`: The placeholder tab the read was started for
    /// - `path`: The file that was read
    /// - `outcome`: The classified result of the read
    /// - `disk_metadata`: The file's metadata taken just before the read
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    fn apply_placeholder_file_read(
        &mut self,
        tab_id: TabId,
        path: &Path,
        outcome: FileReadOutcome,
        disk_metadata: Option<&Metadata>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inflight_content_loads.remove(&tab_id);
        let Some(tab_entity) = self.tab_entity_of(tab_id, cx) else {
            return;
        };
        let still_awaiting = tab_entity.read(cx).as_editor().is_some_and(|editor| {
            editor.awaiting_content && editor.file_path().is_some_and(|tab_path| tab_path == path)
        });
        if !still_awaiting {
            return;
        }
        match outcome {
            FileReadOutcome::Decoded(decoded) => {
                self.fill_placeholder_tab(&tab_entity, path, decoded, disk_metadata, window, cx);
            }
            FileReadOutcome::Binary => {
                log::warn!("Closing restored tab {}: file looks binary", path.display());
                let notice = binary_file_notice("restore", &notice_file_name(path));
                self.drop_unloadable_placeholder(
                    tab_id,
                    (NotificationType::Warning, notice),
                    window,
                    cx,
                );
            }
            FileReadOutcome::Failed(error) => {
                log::warn!(
                    "Closing restored tab {}: cannot read file: {error}",
                    path.display()
                );
                let notice = read_failure_notice("restore", path, &error);
                self.drop_unloadable_placeholder(
                    tab_id,
                    (NotificationType::Error, notice),
                    window,
                    cx,
                );
            }
        }
    }

    /// Load the decoded file into a placeholder tab and unlock it.
    ///
    /// ### Arguments
    /// - `tab_entity`: The placeholder tab
    /// - `path`: The file that was read
    /// - `decoded`: The decoded file contents
    /// - `disk_metadata`: The file's metadata taken just before the read
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    fn fill_placeholder_tab(
        &mut self,
        tab_entity: &Entity<Tab>,
        path: &Path,
        decoded: DecodedContents,
        disk_metadata: Option<&Metadata>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let disk_modified = disk_metadata.and_then(|metadata| metadata.modified().ok());
        let opens_log_view = tab_entity.update(cx, |tab, cx| {
            tab.load_disk_contents(decoded, disk_modified, window, cx);
            let Some(editor_tab) = tab.as_editor_mut() else {
                return false;
            };
            // The placeholder sniffed its delimiter from an empty buffer.
            if editor_tab.language == SupportedLanguage::Csv && !editor_tab.large_file {
                let text = editor_tab.content.read(cx).text().to_string();
                (editor_tab.csv_view_mode, editor_tab.csv_delimiter) =
                    initial_csv_state(editor_tab.language, &text);
            }
            editor_tab.finish_awaiting_content(cx);
            cx.notify();
            editor_tab.log_view
        });
        let tab_id = tab_entity.read(cx).id();
        if let Some(disk_metadata) = disk_metadata {
            self.record_known_disk_state(path, disk_metadata, window, cx);
        }
        if opens_log_view && self.active_tab_id == Some(tab_id) {
            self.activate_log_view(tab_id, window, cx);
        }
        cx.notify();
    }

    /// Close a placeholder tab whose file cannot be shown, and tell the user why.
    ///
    /// A window left without tabs gets a fresh untitled one, as a restore that
    /// produced no tab does.
    ///
    /// ### Arguments
    /// - `tab_id`: The placeholder tab to close
    /// - `notice`: The notification kind and text explaining the removal
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    fn drop_unloadable_placeholder(
        &mut self,
        tab_id: TabId,
        notice: (NotificationType, String),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.remove_tab_by_id(tab_id, window, cx);
        if self.tabs.is_empty() {
            self.new_tab(window, cx);
        } else {
            self.save_state_async(cx, window);
        }
        let (kind, text) = notice;
        window.push_notification((kind, SharedString::from(text)), cx);
    }
}
