use super::super::{DecodedContents, detect_encoding_and_decode, looks_binary};
use crate::fulgur::{
    Fulgur,
    editor_tab::{EditorTab, FromFileParams},
    tab::Tab,
    ui::menus,
};
use gpui::{AsyncWindowContext, Context, SharedString, WeakEntity, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::Path;

/// Result of reading and classifying a file on the background executor.
enum FileReadOutcome {
    Decoded(DecodedContents),
    Binary,
    Failed,
}

impl Fulgur {
    /// Focus an existing tab for a local path and resolve modified-content conflicts.
    ///
    /// ### Arguments
    /// - `path`: The path being opened again
    /// - `tab_index`: The index of the existing tab for this path
    /// - `window`: The active window context
    /// - `cx`: The application context
    pub(super) fn focus_existing_local_tab_for_open(
        &mut self,
        path: &Path,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let is_modified = self
            .tabs
            .get(tab_index)
            .and_then(|tab| tab.read(cx).as_editor())
            .is_some_and(|editor_tab| editor_tab.modified);

        if is_modified {
            log::debug!(
                "Tab for {} has unsaved changes; asking user which version to keep",
                path.display()
            );
            if let Some(tab_id) = self.tabs.get(tab_index).map(|tab| tab.read(cx).id()) {
                self.show_reopen_modified_file_dialog(path, tab_id, window, cx);
            }
        } else {
            log::debug!(
                "Tab for {} is already open and not modified; focusing existing tab",
                path.display()
            );
        }

        self.active_tab_id = self.tabs.get(tab_index).map(|tab| tab.read(cx).id());
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Internal helper function to open a file from a path. This function handles reading the file, detecting encoding, and creating the editor tab
    ///
    /// ### Arguments
    /// - `view`: The view entity (`WeakEntity`)
    /// - `window`: The async window context
    /// - `path`: The path to the file to open
    ///
    /// ### Returns
    /// - `None`: If the file could not be opened
    /// - `Some(())`: If the file was opened successfully
    pub(super) async fn open_file_from_path(
        view: &WeakEntity<Self>,
        window: &mut AsyncWindowContext,
        path: &Path,
    ) -> Option<()> {
        log::debug!("Attempting to open file: {}", path.display());
        let canonical_path = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        let file_size = std::fs::metadata(&canonical_path).map_or(0, |metadata| metadata.len());
        if file_size > crate::fulgur::ui::tabs::editor_tab::LARGE_FILE_THRESHOLD_BYTES {
            let file_name = canonical_path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("file")
                .to_string();
            window
                .update(|window, cx| {
                    window.push_notification(
                        (
                            NotificationType::Info,
                            SharedString::from(format!(
                                "Opening large file '{file_name}', this may take a moment..."
                            )),
                        ),
                        cx,
                    );
                })
                .ok();
        }
        let read_path = canonical_path.clone();
        let outcome = window
            .background_executor()
            .spawn(async move {
                match std::fs::read(&read_path) {
                    Ok(bytes) => {
                        log::debug!(
                            "Successfully read file: {} ({} bytes)",
                            read_path.display(),
                            bytes.len()
                        );
                        if looks_binary(&bytes) {
                            FileReadOutcome::Binary
                        } else {
                            FileReadOutcome::Decoded(detect_encoding_and_decode(bytes))
                        }
                    }
                    Err(e) => {
                        log::error!("Failed to read file {}: {e}", read_path.display());
                        FileReadOutcome::Failed
                    }
                }
            })
            .await;
        let path = canonical_path.as_path();
        let decoded = match outcome {
            FileReadOutcome::Decoded(decoded) => decoded,
            FileReadOutcome::Failed => return None,
            FileReadOutcome::Binary => {
                log::warn!("Refusing to open binary file: {}", path.display());
                let file_name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("file")
                    .to_string();
                window
                    .update(|window, cx| {
                        window.push_notification(
                            (
                                NotificationType::Warning,
                                SharedString::from(format!(
                                    "Cannot open '{file_name}': appears to be a binary file"
                                )),
                            ),
                            cx,
                        );
                    })
                    .ok();
                return None;
            }
        };
        window
            .update(|window, cx| {
                _ = view.update(cx, |this, cx| {
                    let new_tab_id = this.allocate_tab_id();
                    let mut editor_tab = EditorTab::from_file(
                        FromFileParams {
                            id: new_tab_id,
                            path: path.to_path_buf(),
                            contents: decoded.content,
                            encoding: decoded.encoding,
                            is_modified: false,
                        },
                        window,
                        cx,
                        &this.settings.editor_settings,
                    );
                    editor_tab.lossy_decode = decoded.lossy;
                    let editor_tab_index =
                        this.place_editor_tab_reusing_scratch(Tab::Editor(editor_tab), window, cx);
                    this.maybe_open_markdown_preview_for_editor(editor_tab_index, cx);
                    this.watch_file(path);
                    if crate::fulgur::ui::log_view::opens_as_log_by_default(path)
                        && let Some(tab_id) =
                            this.tabs.get(editor_tab_index).map(|tab| tab.read(cx).id())
                    {
                        this.activate_log_view(tab_id, window, cx);
                    }
                    this.focus_active_tab(window, cx);
                    if let Err(e) = this.settings.add_file(path.to_path_buf()) {
                        log::error!("Failed to add file to recent files: {e}");
                    }
                    let shared = Fulgur::shared_state(cx);
                    let update_info = shared.update_info.lock().clone();
                    let update_link = update_info.as_ref().map(|info| info.download_url.clone());
                    let menus = menus::build_menus(
                        &this.settings.get_recent_files(),
                        update_link.as_deref(),
                    );
                    this.update_menus(menus, cx);
                    let title = path.file_name().map(|file_name| {
                        SharedString::from(file_name.to_string_lossy().to_string())
                    });
                    this.set_title(title, cx);
                    log::debug!("File opened successfully in new tab: {}", path.display());
                    this.save_state_async(cx, window);
                    cx.notify();
                });
            })
            .ok();
        Some(())
    }
}
