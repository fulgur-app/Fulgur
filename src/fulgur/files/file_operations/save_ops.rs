use super::{EncodedContents, encode_for_save};
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED,
    utils::atomic_write::atomic_write_file,
};
use gpui::{Context, Focusable, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::Path;

impl Fulgur {
    /// Save a file
    ///
    /// ### Arguments
    /// - `window`: The window to save the file in
    /// - `cx`: The application context
    pub fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let active_tab = &self.tabs[active_tab_index];
        let (tab_id, location, content_entity, encoding, lossy_decode) = match active_tab {
            Tab::Editor(editor_tab) => (
                editor_tab.id,
                editor_tab.location.clone(),
                editor_tab.content.clone(),
                editor_tab.encoding.clone(),
                editor_tab.lossy_decode,
            ),
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        if matches!(location, TabLocation::Untitled) {
            self.save_file_as(window, cx);
            return;
        }
        let contents = content_entity.read(cx).text().to_string();
        // Re-encode using the tab's stored encoding so legacy-encoded files are
        // not silently rewritten as UTF-8. A lossy decode (undecodable bytes
        // already replaced) or an encoding that cannot represent the current
        // text both require the user to confirm a UTF-8 conversion first.
        let bytes = if lossy_decode {
            None
        } else {
            match encode_for_save(&contents, &encoding) {
                EncodedContents::Encoded(bytes) => Some(bytes),
                EncodedContents::Lossy => None,
            }
        };
        let Some(bytes) = bytes else {
            self.show_lossy_save_dialog(tab_id, &encoding, window, cx);
            return;
        };
        match location {
            TabLocation::Local(path) => {
                log::debug!("Saving file: {} ({} bytes)", path.display(), bytes.len());
                if let Err(e) = atomic_write_file(&path, &bytes) {
                    log::error!("Failed to save file {}: {e}", path.display());
                    let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
                    window.push_notification(
                        (
                            NotificationType::Error,
                            SharedString::from(format!("Failed to save '{file_name}': {e}")),
                        ),
                        cx,
                    );
                    return;
                }
                log::debug!("File saved successfully: {}", path.display());
                self.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), std::time::Instant::now());
                if let Tab::Editor(editor_tab) = &mut self.tabs[active_tab_index] {
                    editor_tab.mark_as_saved(cx);
                    editor_tab.update_file_tooltip_cache(bytes.len());
                }
                cx.notify();
            }
            TabLocation::Remote(spec) => {
                self.save_remote_file(window, cx, tab_id, spec, contents, bytes);
            }
            TabLocation::Untitled => {}
        }
    }

    /// Save a file as
    ///
    /// ### Arguments
    /// - `window`: The window to save the file as in
    /// - `cx`: The application context
    pub fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let (tab_id, encoding, directory, suggested_filename) = match &self.tabs[active_tab_index] {
            Tab::Editor(editor_tab) => {
                let dir = if let Some(path) = editor_tab.file_path() {
                    path.parent()
                        .unwrap_or(std::path::Path::new("."))
                        .to_path_buf()
                } else {
                    std::env::current_dir().unwrap_or_default()
                };
                let suggested = editor_tab.get_suggested_filename();
                (editor_tab.id, editor_tab.encoding.clone(), dir, suggested)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let path_future = cx.prompt_for_new_path(&directory, suggested_filename.as_deref());
        cx.spawn_in(window, async move |view, window| {
            let path = path_future.await.ok()?.ok()??;
            let contents = window
                .update(|_, cx| {
                    view.update(cx, |this, cx| {
                        this.tabs
                            .iter()
                            .find(|tab| tab.id() == tab_id)
                            .and_then(|tab| tab.as_editor())
                            .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    })
                    .ok()
                    .flatten()
                })
                .ok()??;
            // Re-encode with the source tab's encoding. If the text cannot be represented, defer to a confirm dialog instead of writing.
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| match encode_for_save(&contents, &encoding) {
                        EncodedContents::Encoded(bytes) => {
                            this.finalize_save_as(
                                tab_id,
                                &path,
                                &bytes,
                                encoding.clone(),
                                window,
                                cx,
                            );
                        }
                        EncodedContents::Lossy => {
                            this.show_lossy_save_as_dialog(
                                tab_id,
                                path.clone(),
                                contents.clone(),
                                &encoding,
                                window,
                                cx,
                            );
                        }
                    });
                })
                .ok()?;
            Some(())
        })
        .detach();
    }

    /// Write a "Save as" result to disk and update the originating tab on success.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab that started `save_file_as`
    /// - `path`: The chosen destination path
    /// - `bytes`: The already-encoded file contents
    /// - `encoding`: The encoding label the bytes were written in
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn finalize_save_as(
        &mut self,
        tab_id: usize,
        path: &Path,
        bytes: &[u8],
        encoding: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::debug!("Saving file as: {} ({} bytes)", path.display(), bytes.len());
        if let Err(e) = atomic_write_file(path, bytes) {
            log::error!("Failed to save file {}: {e}", path.display());
            let file_name = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
            self.pending_notification = Some((
                NotificationType::Error,
                SharedString::from(format!("Failed to save '{file_name}': {e}")),
            ));
            cx.notify();
            return;
        }
        log::debug!("File saved successfully as: {}", path.display());
        let Some(tab_index) = self.tabs.iter().position(|tab| tab.id() == tab_id) else {
            log::warn!("Save As destination selected, but tab {tab_id} no longer exists");
            return;
        };
        let old_path = self
            .tabs
            .get(tab_index)
            .and_then(Tab::as_editor)
            .and_then(|editor_tab| editor_tab.file_path().cloned());
        if let Some(old_path) = old_path {
            self.unwatch_file(&old_path);
        }
        self.file_watch_state
            .last_file_saves
            .insert(path.to_path_buf(), std::time::Instant::now());
        if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
            editor_tab.location = TabLocation::Local(path.to_path_buf());
            editor_tab.title = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(UNTITLED)
                .to_string()
                .into();
            editor_tab.encoding = encoding;
            editor_tab.mark_as_saved(cx);
            editor_tab.update_file_tooltip_cache(bytes.len());
            editor_tab.update_language(window, cx, &self.settings.editor_settings);
            cx.notify();
        }
        self.watch_file(path);
    }

    /// Show notification when file is reloaded
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was reloaded
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_reloaded(
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File {filename} has been updated externally"));
        window.push_notification((NotificationType::Info, message), cx);
    }

    /// Show notification when file is deleted
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was deleted
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_deleted(
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File '{filename}' deleted externally"));
        window.push_notification((NotificationType::Warning, message), cx);
    }

    /// Show notification when file is renamed
    ///
    /// ### Arguments
    /// - `from`: The path to the file that was renamed from
    /// - `to`: The path to the file that was renamed to
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_renamed(
        from: &Path,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_name = from.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let new_name = to.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File renamed: {old_name} → {new_name}"));
        window.push_notification((NotificationType::Info, message), cx);
    }

    /// Update search results if the search query has changed
    ///
    /// ### Arguments
    /// - `window`: The window containing the search bar and editor
    /// - `cx`: The application context
    pub fn update_search_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.show_search {
            let current_query = self.search_state.search_input.read(cx).text().to_string();
            if current_query != self.search_state.last_search_query {
                self.perform_search(window, cx);
                // Restore focus to the search input after perform_search
                let search_focus = self.search_state.search_input.read(cx).focus_handle(cx);
                window.focus(&search_focus, cx);
            }
        }
    }

    /// Open the native OS print dialog for the current document
    ///
    /// Writes the active tab's content to a temporary HTML file and opens it with
    /// the system's default browser, which automatically triggers the native print dialog.
    /// This approach works cross-platform without requiring OS-specific print APIs.
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn print_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(active_tab_index) = self.active_tab_index else {
            return;
        };
        let (title, content) = match &self.tabs[active_tab_index] {
            Tab::Editor(editor_tab) => {
                let title = editor_tab.title.clone();
                let content = editor_tab.content.read(cx).text().to_string();
                (title, content)
            }
            Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
        };
        let escaped_content = content
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let escaped_title = title
            .replace('&', "&amp;")
            .replace('<', "&lt;")
            .replace('>', "&gt;");
        let html = format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<title>{escaped_title}</title>
<style>
  body {{ margin: 0; padding: 1em; font-family: monospace; white-space: pre-wrap; word-wrap: break-word; }}
  @media print {{ body {{ margin: 0; }} }}
</style>
</head>
<body>{escaped_content}</body>
<script>window.onload = function() {{ window.print(); }};</script>
</html>"#,
        );
        let temp_path =
            std::env::temp_dir().join(format!("fulgur_print_{}.html", std::process::id()));
        if let Err(e) = std::fs::write(&temp_path, html.as_bytes()) {
            log::error!("Failed to write print temp file: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to prepare print: {e}")),
                ),
                cx,
            );
            return;
        }
        if let Err(e) = open::that(&temp_path) {
            log::error!("Failed to open print file: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to open print dialog: {e}")),
                ),
                cx,
            );
        }
    }

    /// Handle pending jump-to-line action
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    pub fn handle_pending_jump_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(jump) = self.pending_jump.take()
            && let Some(index) = self.active_tab_index
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.jump_to_line(window, cx, jump);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::setup_fulgur;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{editor_tab::TabLocation, tab::Tab};
    #[cfg(feature = "gpui-test-support")]
    use gpui::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    // ========== save_file tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_writes_content_to_disk(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("save_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                }
                this.save_file(window, cx);
            });
        });

        assert!(path.exists(), "file should exist after save_file");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_marks_tab_as_not_modified(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("mark_saved_test.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.modified = true;
                }
                this.save_file(window, cx);
                let modified = this
                    .tabs
                    .last()
                    .and_then(|t| t.as_editor())
                    .is_none_or(|e| e.modified);
                assert!(!modified, "tab should be marked as not modified after save");
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.save_file(window, cx); // Must not panic
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_save_file_preserves_non_utf8_encoding(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("latin1.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.encoding = "windows-1252".to_string();
                    editor_tab.content.update(cx, |state, cx| {
                        state.set_value("café", window, cx);
                    });
                }
                this.save_file(window, cx);
            });
        });

        let bytes = std::fs::read(&path).expect("file should exist after save");
        // "café" must be written as the single windows-1252 byte 0xE9, not the
        // UTF-8 two-byte sequence 0xC3 0xA9.
        assert_eq!(bytes, vec![0x63, 0x61, 0x66, 0xE9]);
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_finalize_save_as_targets_tab_by_id(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let first_path = dir.path().join("first.txt");
        let renamed_path = dir.path().join("renamed.rs");
        let second_path = dir.path().join("second.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let first_tab_id = this.tabs.first().expect("expected first tab").id();
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.location = TabLocation::Local(first_path.clone());
                }

                this.new_tab(window, cx);
                let second_tab_id = this.tabs.last().expect("expected second tab").id();
                if let Some(Tab::Editor(editor_tab)) = this.tabs.last_mut() {
                    editor_tab.location = TabLocation::Local(second_path.clone());
                }

                this.finalize_save_as(
                    first_tab_id,
                    &renamed_path,
                    b"hello",
                    "UTF-8".to_string(),
                    window,
                    cx,
                );

                let first_tab_path = this
                    .tabs
                    .iter()
                    .find(|tab| tab.id() == first_tab_id)
                    .and_then(|tab| tab.as_editor())
                    .and_then(|editor_tab| editor_tab.file_path().cloned())
                    .expect("first tab path should exist");
                let second_tab_path = this
                    .tabs
                    .iter()
                    .find(|tab| tab.id() == second_tab_id)
                    .and_then(|tab| tab.as_editor())
                    .and_then(|editor_tab| editor_tab.file_path().cloned())
                    .expect("second tab path should exist");

                assert_eq!(
                    first_tab_path, renamed_path,
                    "save-as update must target the originating tab id"
                );
                assert_eq!(
                    second_tab_path, second_path,
                    "save-as update must not alter other tabs"
                );
            });
        });
    }
}
