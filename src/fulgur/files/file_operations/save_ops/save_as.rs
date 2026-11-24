use super::completion::{SaveCompletion, SavedSnapshot};
use super::snapshot::{EncodedSave, SaveSnapshot};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, tab::Tab, ui::components_utils::UNTITLED,
    utils::atomic_write::atomic_write_file,
};
use gpui_kit::{App, Context, Window};
use std::fs::Metadata;
use std::path::{Path, PathBuf};

impl Fulgur {
    /// Save a file as
    ///
    /// ### Arguments
    /// - `window`: The window to save the file as in
    /// - `cx`: The application context
    pub fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.refuse_save_of_unloaded_tab(window, cx) {
            return;
        }
        let Some(editor_tab) = self.active_tab(cx).and_then(Tab::as_editor) else {
            return;
        };
        let (tab_id, encoding) = (editor_tab.id, editor_tab.encoding.clone());
        let Some(path_future) = self.prompt_for_save_as_path(tab_id, cx) else {
            return;
        };
        cx.spawn_in(window, async move |view, window| {
            let path = path_future.await?;
            let snapshot = window
                .update(|_, cx| {
                    view.update(cx, |this, cx| {
                        let tab = this.tab_entity_of(tab_id, cx)?;
                        let editor_tab = tab.read(cx).as_editor()?;
                        Some(SaveSnapshot::capture(editor_tab, cx))
                    })
                    .ok()
                    .flatten()
                })
                .ok()??;
            // Re-encode with the source tab's encoding. If the text cannot be represented, defer to a confirm dialog instead of writing.
            let prepare_encoding = encoding.clone();
            let prepared = window
                .background_executor()
                .spawn(async move { snapshot.prepare(&prepare_encoding) })
                .await;
            let fingerprint = prepared.fingerprint;
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| match prepared.encoded {
                        Ok(bytes) => {
                            let encoded = EncodedSave {
                                bytes,
                                encoding,
                                fingerprint,
                            };
                            this.finalize_save_as(tab_id, &path, encoded, window, cx);
                        }
                        Err(contents) => {
                            Self::show_lossy_save_as_dialog(
                                tab_id,
                                path,
                                (contents, fingerprint),
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

    /// Ask the user where to save an editor tab, starting next to its current file.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab to save
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Future)`: Resolves to the chosen path, or `None` when the user cancels
    /// - `None`: The tab is missing or is not an editor tab
    pub(crate) fn prompt_for_save_as_path(
        &self,
        tab_id: TabId,
        cx: &App,
    ) -> Option<impl Future<Output = Option<PathBuf>> + use<>> {
        let tab = self.tab_entity_of(tab_id, cx)?;
        let editor_tab = tab.read(cx).as_editor()?;
        let directory = match editor_tab.file_path() {
            Some(path) => path.parent().unwrap_or(Path::new(".")).to_path_buf(),
            None => std::env::current_dir().unwrap_or_default(),
        };
        let suggested_filename = editor_tab.get_suggested_filename();
        let receiver = cx.prompt_for_new_path(&directory, suggested_filename.as_deref());
        Some(async move { receiver.await.ok()?.ok()? })
    }

    /// Point a tab at the local file it was just saved to, and watch that file
    /// instead of its previous one.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the saved editor tab
    /// - `path`: The canonical path the tab was written to
    /// - `cx`: The application context
    pub(crate) fn retarget_tab_to_saved_file(
        &mut self,
        tab_id: TabId,
        path: &Path,
        cx: &mut Context<Self>,
    ) {
        let old_path = self
            .tab_entity_of(tab_id, cx)
            .and_then(|tab| tab.read(cx).as_editor()?.file_path().cloned());
        if let Some(old_path) = old_path {
            self.unwatch_file(&old_path);
        }
        self.update_editor_tab(tab_id, cx, |editor_tab, cx| {
            editor_tab.location = TabLocation::Local(path.to_path_buf());
            editor_tab.title = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or(UNTITLED)
                .to_string()
                .into();
            editor_tab.update_language(cx);
            cx.notify();
        });
        cx.notify();
        self.watch_file(path);
        crate::fulgur::state::schedule_session_autosave(cx);
    }

    /// Write a "Save as" result to disk and update the originating tab on success.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab that started `save_file_as`
    /// - `path`: The chosen destination path
    /// - `encoded`: The encoded bytes, their encoding label, and the identity of
    ///   the editor text they encode
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn finalize_save_as(
        &mut self,
        tab_id: TabId,
        path: &Path,
        encoded: EncodedSave,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let EncodedSave {
            bytes,
            encoding,
            fingerprint,
        } = encoded;
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
            path: path.to_path_buf(),
        };
        let saved = SavedSnapshot {
            byte_len: bytes.len(),
            fingerprint,
        };
        cx.spawn_in(window, async move |view, window| {
            let write_path = completion.path.clone();
            let write_result = window
                .background_executor()
                .spawn(async move {
                    atomic_write_file(&write_path, &bytes).map(|written| {
                        let canonical = std::fs::canonicalize(&write_path)
                            .unwrap_or_else(|_| write_path.clone());
                        (canonical, written)
                    })
                })
                .await;
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        this.finish_save_as(
                            &completion,
                            (saved, encoding),
                            write_result,
                            window,
                            cx,
                        );
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
    /// - `written`: The editor snapshot the bytes encode, and the encoding label
    ///   they were written in
    /// - `write_result`: The canonicalized destination path with the metadata of
    ///   the written file, or the write error
    /// - `window`: The window context
    /// - `cx`: The application context
    fn finish_save_as(
        &mut self,
        completion: &SaveCompletion,
        written: (SavedSnapshot, String),
        write_result: anyhow::Result<(PathBuf, Metadata)>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let tab_id = completion.tab_id;
        let (saved, encoding) = written;
        self.inflight_saves.remove(&tab_id);
        match write_result {
            Ok((canonical_path, written)) => {
                let path = canonical_path.as_path();
                log::debug!("File saved successfully as: {}", path.display());
                if self.tab_entity_of(tab_id, cx).is_some() {
                    self.update_editor_tab(tab_id, cx, |editor_tab, cx| {
                        editor_tab.encoding = encoding;
                        saved.record_on(editor_tab, &written, cx);
                    });
                    self.retarget_tab_to_saved_file(tab_id, path, cx);
                    // After the retarget, so the new path is already watched.
                    self.record_known_disk_state(path, &written, window, cx);
                } else {
                    log::warn!("Save As completed, but tab {tab_id} no longer exists");
                }
                self.resume_after_local_save(tab_id, window, cx);
            }
            Err(e) => {
                self.cancel_close_after_failed_local_save(tab_id);
                Self::handle_failed_save(completion, &e, window, cx);
            }
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use super::{EncodedSave, SaveSnapshot};
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::setup_fulgur;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{editor_tab::TabLocation, tab::Tab, ui::components_utils::UTF_8};
    #[cfg(feature = "gpui-test-support")]
    use gpui_kit::TestAppContext;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
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
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(first_path.clone());
                            editor_tab.content.update(cx, |state, cx| {
                                state.set_value("hello", window, cx);
                            });
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

                let prepared = this
                    .tabs
                    .first()
                    .and_then(|tab| tab.read(cx).as_editor())
                    .map(|editor_tab| SaveSnapshot::capture(editor_tab, cx).prepare(UTF_8))
                    .expect("expected first editor tab");
                let encoded = EncodedSave {
                    bytes: prepared.encoded.expect("UTF-8 encodes any text"),
                    encoding: UTF_8.to_string(),
                    fingerprint: prepared.fingerprint,
                };
                this.finalize_save_as(first_tab_id, &renamed_path, encoded, window, cx);
                let first_editor = this
                    .tabs
                    .first()
                    .and_then(|tab| tab.read(cx).as_editor())
                    .expect("expected first editor tab");
                assert!(
                    first_editor.content_differs_from_original(cx),
                    "Save As must retain the old baseline while its write is pending"
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
                let first_editor = this
                    .tabs
                    .iter()
                    .map(|tab| tab.read(cx))
                    .find(|tab| tab.id() == first_tab_id)
                    .and_then(Tab::as_editor)
                    .expect("expected first editor tab");
                assert!(
                    !first_editor.content_differs_from_original(cx),
                    "Save As should advance the baseline after the write succeeds"
                );
            });
        });
    }
}
