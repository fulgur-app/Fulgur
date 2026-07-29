use super::super::{EncodedContents, encode_for_save};
use super::completion::SaveCompletion;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, tab::Tab, utils::atomic_write::atomic_write_file,
};
use gpui::{Context, Window};
use std::path::PathBuf;

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
        // not silently rewritten as UTF-8.
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
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::editor_tab::TabLocation;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::setup_fulgur;
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
}
