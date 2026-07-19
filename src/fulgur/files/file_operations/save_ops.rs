use super::{EncodedContents, encode_for_save};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED,
    utils::atomic_write::atomic_write_file,
};
use gpui::{Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::{Path, PathBuf};

/// Snapshot of a tab's saved-content baseline, captured before an optimistic
/// save so a failed background write can restore it.
struct SavedBaseline {
    /// The tab's `original_content_hash` at dispatch time
    hash: u64,
    /// The tab's `original_content_len` at dispatch time
    len: usize,
    /// The tab's `modified` flag at dispatch time
    modified: bool,
}

/// Dispatch-time context of a background save, handed back to the completion
/// handler that runs on the UI thread once the write resolves.
struct SaveCompletion {
    /// Stable identifier of the editor tab being saved
    tab_id: TabId,
    /// Destination path of the write, as requested at dispatch
    path: PathBuf,
    /// Size of the written content in bytes
    byte_len: usize,
    /// Saved baseline captured at dispatch, restored if the write fails
    previous_baseline: Option<SavedBaseline>,
}

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
        let Some(active_tab_index) = self.active_tab_index(cx) else {
            return;
        };
        let active_tab = self.tabs[active_tab_index].read(cx);
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
                self.spawn_local_save(tab_id, path, bytes, window, cx);
            }
            TabLocation::Remote(spec) => {
                self.save_remote_file(window, cx, tab_id, spec, contents, bytes);
            }
            TabLocation::Untitled => {}
        }
    }

    /// Dispatch a background atomic write of a local tab's encoded content.
    ///
    /// ### Description
    /// The disk write (with its fsyncs) runs on the background executor so the
    /// UI thread never blocks on I/O. The tab is optimistically marked as saved
    /// at dispatch time, when the buffer still matches the written snapshot;
    /// the tab's content subscription flips `modified` back on any edit made
    /// while the write is in flight, and a failed write restores the previous
    /// saved baseline. A tab with a save already in flight is skipped to keep
    /// writes for one file strictly ordered. Registering the destination in
    /// `inflight_saves` also suppresses the watcher echo of the write.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab being saved
    /// - `path`: Destination path of the local file
    /// - `bytes`: The already-encoded file contents
    /// - `window`: The window context
    /// - `cx`: The application context
    fn spawn_local_save(
        &mut self,
        tab_id: TabId,
        path: PathBuf,
        bytes: Vec<u8>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inflight_saves.contains_key(&tab_id) {
            log::debug!(
                "Save already in flight for {}; skipping duplicate save",
                path.display()
            );
            return;
        }
        log::debug!("Saving file: {} ({} bytes)", path.display(), bytes.len());
        self.inflight_saves.insert(tab_id, path.clone());
        let completion = SaveCompletion {
            tab_id,
            byte_len: bytes.len(),
            previous_baseline: self.capture_saved_baseline(tab_id, cx),
            path,
        };
        self.update_editor_tab(tab_id, cx, |editor_tab, cx| {
            editor_tab.mark_as_saved(cx);
            cx.notify();
        });
        cx.notify();
        cx.spawn_in(window, async move |view, window| {
            let write_path = completion.path.clone();
            let write_result = window
                .background_executor()
                .spawn(async move { atomic_write_file(&write_path, &bytes) })
                .await;
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        this.finish_local_save(completion, write_result, window, cx);
                    });
                })
                .ok();
        })
        .detach();
    }

    /// Apply the outcome of a background local save back on the UI thread.
    ///
    /// ### Arguments
    /// - `completion`: Dispatch-time context of the save being completed
    /// - `write_result`: The result of the background `atomic_write_file` call
    /// - `window`: The window context
    /// - `cx`: The application context
    fn finish_local_save(
        &mut self,
        completion: SaveCompletion,
        write_result: anyhow::Result<()>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inflight_saves.remove(&completion.tab_id);
        match write_result {
            Ok(()) => {
                log::debug!("File saved successfully: {}", completion.path.display());
                // For Inode-based backends (Linux inotify, BSD kqueue).
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    self.unwatch_file(&completion.path);
                    self.watch_file(&completion.path);
                }
                self.file_watch_state
                    .last_file_saves
                    .insert(completion.path.clone(), std::time::Instant::now());
                let byte_len = completion.byte_len;
                self.update_editor_tab(completion.tab_id, cx, |editor_tab, cx| {
                    editor_tab.update_file_tooltip_cache(byte_len);
                    cx.notify();
                });
                cx.notify();
            }
            Err(e) => {
                self.handle_failed_save(completion, &e, window, cx);
            }
        }
    }

    /// Capture a tab's saved-content baseline before an optimistic save.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(SavedBaseline)`: The tab's current baseline and modified flag
    /// - `None`: If the tab no longer exists or is not an editor tab
    fn capture_saved_baseline(&self, tab_id: TabId, cx: &Context<Self>) -> Option<SavedBaseline> {
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
    fn handle_failed_save(
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

    /// Save a file as
    ///
    /// ### Arguments
    /// - `window`: The window to save the file as in
    /// - `cx`: The application context
    pub fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }
        let Some(active_tab_index) = self.active_tab_index(cx) else {
            return;
        };
        let (tab_id, encoding, directory, suggested_filename) =
            match self.tabs[active_tab_index].read(cx) {
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
                            .map(|tab| tab.read(cx))
                            .find(|tab| tab.id() == tab_id)
                            .and_then(Tab::as_editor)
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
        tab_id: TabId,
        path: &Path,
        bytes: &[u8],
        encoding: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inflight_saves.contains_key(&tab_id) {
            log::debug!(
                "Save already in flight for tab {tab_id}; skipping Save As to {}",
                path.display()
            );
            return;
        }
        log::debug!("Saving file as: {} ({} bytes)", path.display(), bytes.len());
        self.inflight_saves.insert(tab_id, path.to_path_buf());
        let completion = SaveCompletion {
            tab_id,
            byte_len: bytes.len(),
            previous_baseline: self.capture_saved_baseline(tab_id, cx),
            path: path.to_path_buf(),
        };
        self.update_editor_tab(tab_id, cx, |editor_tab, cx| {
            editor_tab.mark_as_saved(cx);
            cx.notify();
        });
        let bytes = bytes.to_vec();
        cx.spawn_in(window, async move |view, window| {
            let write_path = completion.path.clone();
            let write_result = window
                .background_executor()
                .spawn(async move {
                    atomic_write_file(&write_path, &bytes).map(|()| {
                        std::fs::canonicalize(&write_path).unwrap_or_else(|_| write_path.clone())
                    })
                })
                .await;
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        this.finish_save_as(completion, encoding, write_result, window, cx);
                    });
                })
                .ok();
        })
        .detach();
    }

    /// Apply the outcome of a background "Save as" write back on the UI thread.
    ///
    /// ### Arguments
    /// - `completion`: Dispatch-time context of the save being completed
    /// - `encoding`: The encoding label the bytes were written in
    /// - `write_result`: The canonicalized destination path, or the write error
    /// - `window`: The window context
    /// - `cx`: The application context
    fn finish_save_as(
        &mut self,
        completion: SaveCompletion,
        encoding: String,
        write_result: anyhow::Result<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = completion.tab_id;
        let byte_len = completion.byte_len;
        self.inflight_saves.remove(&tab_id);
        match write_result {
            Ok(canonical_path) => {
                let path = canonical_path.as_path();
                log::debug!("File saved successfully as: {}", path.display());
                let Some(tab_entity) = self.tab_entity_of(tab_id, cx) else {
                    log::warn!("Save As completed, but tab {tab_id} no longer exists");
                    return;
                };
                let old_path = tab_entity
                    .read(cx)
                    .as_editor()
                    .and_then(|editor_tab| editor_tab.file_path().cloned());
                if let Some(old_path) = old_path {
                    self.unwatch_file(&old_path);
                }
                self.file_watch_state
                    .last_file_saves
                    .insert(path.to_path_buf(), std::time::Instant::now());
                let settings = self.settings.editor_settings.clone();
                tab_entity.update(cx, |tab, cx| {
                    let Some(editor_tab) = tab.as_editor_mut() else {
                        return;
                    };
                    editor_tab.location = TabLocation::Local(path.to_path_buf());
                    editor_tab.title = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or(UNTITLED)
                        .to_string()
                        .into();
                    editor_tab.encoding = encoding;
                    editor_tab.update_file_tooltip_cache(byte_len);
                    tab.update_language(window, cx, &settings);
                    cx.notify();
                });
                cx.notify();
                self.watch_file(path);
            }
            Err(e) => {
                self.handle_failed_save(completion, &e, window, cx);
            }
        }
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
        let Some(active_tab_index) = self.active_tab_index(cx) else {
            return;
        };
        let (title, content) = match self.tabs[active_tab_index].read(cx) {
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
        if let Some(jump) = self.pending_jump.take() {
            self.update_active_editor_tab(cx, |editor_tab, cx| {
                editor_tab.jump_to_line(window, cx, jump);
            });
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
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                        }
                    });
                this.save_file(window, cx);
            });
        });
        visual_cx.run_until_parked();

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
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.modified = true;
                        }
                    });
                this.save_file(window, cx);
                let modified = this
                    .tabs
                    .last()
                    .and_then(|t| t.read(cx).as_editor())
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
                this.active_tab_id = None;
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
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.encoding = "windows-1252".to_string();
                            editor_tab.content.update(cx, |state, cx| {
                                state.set_value("café", window, cx);
                            });
                        }
                    });
                this.save_file(window, cx);
            });
        });
        visual_cx.run_until_parked();

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

        let (first_tab_id, second_tab_id) = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let first_tab_id = this.tabs.first().expect("expected first tab").read(cx).id();
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(first_path.clone());
                        }
                    });

                this.new_tab(window, cx);
                let second_tab_id = this.tabs.last().expect("expected second tab").read(cx).id();
                this.tabs
                    .last()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(second_path.clone());
                        }
                    });

                this.finalize_save_as(
                    first_tab_id,
                    &renamed_path,
                    b"hello",
                    "UTF-8".to_string(),
                    window,
                    cx,
                );
                (first_tab_id, second_tab_id)
            })
        });
        visual_cx.run_until_parked();

        visual_cx.update(|_, cx| {
            fulgur.update(cx, |this, cx| {
                let first_tab_path = this
                    .tabs
                    .iter()
                    .map(|tab| tab.read(cx))
                    .find(|tab| tab.id() == first_tab_id)
                    .and_then(Tab::as_editor)
                    .and_then(|editor_tab| editor_tab.file_path().cloned())
                    .expect("first tab path should exist");
                let second_tab_path = this
                    .tabs
                    .iter()
                    .map(|tab| tab.read(cx))
                    .find(|tab| tab.id() == second_tab_id)
                    .and_then(Tab::as_editor)
                    .and_then(|editor_tab| editor_tab.file_path().cloned())
                    .expect("second tab path should exist");

                // finalize_save_as canonicalizes the destination, so compare against the
                // resolved path (macOS resolves /var/ to /private/var/).
                let expected_renamed_path =
                    std::fs::canonicalize(&renamed_path).unwrap_or_else(|_| renamed_path.clone());
                assert_eq!(
                    first_tab_path, expected_renamed_path,
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
