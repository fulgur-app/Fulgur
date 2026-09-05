use super::super::{EncodedContents, encode_for_save};
use super::completion::SaveCompletion;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED,
    utils::atomic_write::atomic_write_file,
};
use gpui::{Context, Window};
use std::path::{Path, PathBuf};

impl Fulgur {
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
                    tab.update_language(cx);
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
