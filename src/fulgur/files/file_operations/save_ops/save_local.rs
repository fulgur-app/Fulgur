use super::super::{EncodedContents, RemoteSaveRequest, encode_for_save};
use super::completion::{SaveCompletion, SavedSnapshot};
use super::snapshot::SaveSnapshot;
use crate::fulgur::ui::dialogs::save_conflict::ConflictedFile;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, state::file_modified_system_time, tab::Tab,
    utils::atomic_write::atomic_write_file,
};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{Context, SharedString, Window};
use std::fs::Metadata;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

/// What a save does when the file, on disk or on a remote host, no longer matches
/// the version the buffer is based on.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum DiskChangePolicy {
    /// Refuse the write and ask the user how to resolve the conflict.
    Prompt,
    /// Write anyway, replacing the newer version.
    Overwrite,
}

/// Result of a background local save that did not fail.
pub(super) enum LocalWriteOutcome {
    /// The file was written; holds the metadata of the written version and
    /// the editor snapshot it contains.
    Written(Metadata, SavedSnapshot),
    /// The write was refused because the file changed on disk since the
    /// version the buffer is based on.
    ChangedOnDisk,
    /// Nothing was written because the buffer cannot be represented in the
    /// tab's encoding; holds that encoding label.
    Unencodable(String),
}

/// Destination of a local save and the disk state it expects to replace.
struct LocalSaveTarget {
    /// Destination path of the local file
    path: PathBuf,
    /// Modification time the file must still have for the write to go ahead,
    /// or `None` to write whatever the file on disk is
    disk_baseline: Option<SystemTime>,
}

/// Check whether a local file changed on disk since the version a buffer is based on.
///
/// ### Arguments
/// - `path`: The file about to be written
/// - `baseline`: The modification time of the version the buffer is based on
///   (the tab's `file_last_modified`)
///
/// ### Returns
/// - `true`: The file exists and its modification time differs from `baseline`
/// - `false`: The baseline is unknown, the file is missing, or it still matches
pub(crate) fn disk_changed_since(path: &Path, baseline: Option<SystemTime>) -> bool {
    let Some(baseline) = baseline else {
        return false;
    };
    file_modified_system_time(path).is_some_and(|current| current != baseline)
}

/// Encode a save snapshot and write it to a local file, unless the file changed
/// on disk since the version the buffer is based on. Linear in the buffer size,
/// so it must run on a background executor.
///
/// ### Arguments
/// - `target`: Destination path and the disk state the write expects to replace
/// - `snapshot`: The editor text to write
/// - `encoding`: The encoding label to write the text in
///
/// ### Errors
/// - Returns an `anyhow::Error` if the atomic write fails.
///
/// ### Returns
/// - `Ok(LocalWriteOutcome)`: The file was written, or deliberately left alone
/// - `Err(anyhow::Error)`: The write failed
fn write_local_snapshot(
    target: &LocalSaveTarget,
    snapshot: SaveSnapshot,
    encoding: String,
) -> anyhow::Result<LocalWriteOutcome> {
    let prepared = snapshot.prepare(&encoding);
    let Ok(bytes) = prepared.encoded else {
        return Ok(LocalWriteOutcome::Unencodable(encoding));
    };
    if disk_changed_since(&target.path, target.disk_baseline) {
        return Ok(LocalWriteOutcome::ChangedOnDisk);
    }
    log::debug!(
        "Writing file: {} ({} bytes)",
        target.path.display(),
        bytes.len()
    );
    let saved = SavedSnapshot {
        byte_len: bytes.len(),
        fingerprint: prepared.fingerprint,
    };
    atomic_write_file(&target.path, &bytes)
        .map(|written| LocalWriteOutcome::Written(written, saved))
}

impl Fulgur {
    /// Save the active tab
    ///
    /// ### Arguments
    /// - `window`: The window to save the file in
    /// - `cx`: The application context
    pub fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() || self.refuse_save_of_unloaded_tab(window, cx) {
            return;
        }
        let Some(tab_id) = self
            .active_tab_index(cx)
            .map(|index| self.tabs[index].read(cx).id())
        else {
            return;
        };
        self.save_editor_tab(tab_id, DiskChangePolicy::Prompt, window, cx);
    }

    /// Save an editor tab to its backing location
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the tab to save
    /// - `disk_change_policy`: What the save does if the file changed since it was loaded
    /// - `window`: The window to save the file in
    /// - `cx`: The application context
    pub(crate) fn save_editor_tab(
        &mut self,
        tab_id: TabId,
        disk_change_policy: DiskChangePolicy,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tab_entity_of(tab_id, cx) else {
            return;
        };
        let (location, snapshot, encoding, lossy_decode, read_only, disk_baseline) =
            match tab.read(cx) {
                Tab::Editor(editor_tab) => (
                    editor_tab.location.clone(),
                    SaveSnapshot::capture(editor_tab, cx),
                    editor_tab.encoding.clone(),
                    editor_tab.lossy_decode,
                    editor_tab.read_only,
                    editor_tab.file_last_modified,
                ),
                Tab::Settings(_) | Tab::MarkdownPreview(_) => return,
            };
        if !location.has_backing_file() {
            self.save_file_as(window, cx);
            return;
        }
        if read_only {
            window.push_notification(
                (
                    NotificationType::Info,
                    SharedString::from("This file is read-only. Use Save As to write a copy."),
                ),
                cx,
            );
            return;
        }
        if lossy_decode {
            self.show_lossy_save_dialog(tab_id, &encoding, window, cx);
            return;
        }
        // Both branches re-encode using the tab's stored encoding so
        // legacy-encoded files are not silently rewritten as UTF-8.
        match location {
            TabLocation::Local(path) => {
                let target = LocalSaveTarget {
                    path,
                    disk_baseline: match disk_change_policy {
                        DiskChangePolicy::Prompt => disk_baseline,
                        DiskChangePolicy::Overwrite => None,
                    },
                };
                self.spawn_local_save(tab_id, target, snapshot, encoding, window, cx);
            }
            TabLocation::Remote(spec) => {
                let contents = snapshot.into_string();
                let EncodedContents::Encoded(bytes) = encode_for_save(&contents, &encoding) else {
                    self.show_lossy_save_dialog(tab_id, &encoding, window, cx);
                    return;
                };
                let request = RemoteSaveRequest {
                    tab_id,
                    spec,
                    contents,
                    bytes,
                    disk_change_policy,
                    on_saved: None,
                };
                self.save_remote_file(window, cx, request);
            }
            TabLocation::Untitled | TabLocation::Shared(_) => {}
        }
    }

    /// Dispatch the background encoding and atomic write of a local tab's content.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable identifier of the editor tab being saved
    /// - `target`: Destination path and the disk state the write expects to replace
    /// - `snapshot`: The editor text to write
    /// - `encoding`: The encoding label to write the text in
    /// - `window`: The window context
    /// - `cx`: The application context
    fn spawn_local_save(
        &mut self,
        tab_id: TabId,
        target: LocalSaveTarget,
        snapshot: SaveSnapshot,
        encoding: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.inflight_saves.contains_key(&tab_id) {
            log::debug!(
                "Save already in flight for {}; skipping duplicate save",
                target.path.display()
            );
            return;
        }
        log::debug!("Saving file: {}", target.path.display());
        self.inflight_saves.insert(tab_id, target.path.clone());
        let completion = SaveCompletion {
            tab_id,
            path: target.path.clone(),
        };
        cx.notify();
        cx.spawn_in(window, async move |view, window| {
            let write_result = window
                .background_executor()
                .spawn(async move { write_local_snapshot(&target, snapshot, encoding) })
                .await;
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        this.finish_local_save(&completion, write_result, window, cx);
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
    /// - `write_result`: The outcome of the background disk check and write
    /// - `window`: The window context
    /// - `cx`: The application context
    fn finish_local_save(
        &mut self,
        completion: &SaveCompletion,
        write_result: anyhow::Result<LocalWriteOutcome>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.inflight_saves.remove(&completion.tab_id);
        match write_result {
            Ok(LocalWriteOutcome::Written(written, saved)) => {
                log::debug!("File saved successfully: {}", completion.path.display());
                // For Inode-based backends (Linux inotify, BSD kqueue).
                #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                {
                    self.unwatch_file(&completion.path);
                    self.watch_file(&completion.path);
                }
                self.record_known_disk_state(&completion.path, &written, window, cx);
                self.update_editor_tab(completion.tab_id, cx, |editor_tab, cx| {
                    saved.record_on(editor_tab, &written, cx);
                    cx.notify();
                });
                cx.notify();
                crate::fulgur::state::schedule_session_autosave(cx);
                self.resume_after_local_save(completion.tab_id, window, cx);
            }
            Ok(LocalWriteOutcome::ChangedOnDisk) => {
                log::warn!(
                    "Not saving {}: it changed on disk since it was last loaded or saved",
                    completion.path.display()
                );
                self.cancel_close_after_failed_local_save(completion.tab_id);
                self.show_save_conflict_dialog(
                    ConflictedFile::Local(completion.path.clone()),
                    completion.tab_id,
                    window,
                    cx,
                );
                cx.notify();
            }
            Ok(LocalWriteOutcome::Unencodable(encoding)) => {
                self.cancel_close_after_failed_local_save(completion.tab_id);
                self.show_lossy_save_dialog(completion.tab_id, &encoding, window, cx);
                cx.notify();
            }
            Err(e) => {
                self.cancel_close_after_failed_local_save(completion.tab_id);
                Self::handle_failed_save(completion, &e, window, cx);
            }
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::LocalWriteOutcome;
    #[cfg(feature = "gpui-test-support")]
    use super::SaveCompletion;
    use super::{SaveSnapshot, SavedSnapshot};
    use crate::fulgur::Fulgur;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::PendingSaveCloseAction;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::editor_tab::TabLocation;
    use crate::fulgur::editor_tab::replace_editor_text;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::files::file_operations::test_helpers::{
        setup_fulgur, setup_fulgur_with_root,
    };
    use crate::fulgur::state::file_modified_system_time;
    use crate::fulgur::ui::components_utils::UTF_8;
    use crate::fulgur::ui::tabs::tab::TabId;
    use crate::fulgur::utils::atomic_write::atomic_write_file;
    use crate::test_support::{click_dialog_button, has_active_dialog, write_with_mtime};
    #[cfg(feature = "gpui-test-support")]
    use gpui_kit::TestAppContext;
    use gpui_kit::{App, Entity, VisualTestContext};
    use std::path::Path;
    use std::time::SystemTime;
    #[cfg(feature = "gpui-test-support")]
    use tempfile::TempDir;

    const LOCAL_TEXT: &str = "local edits";
    const EXTERNAL_TEXT: &str = "rewritten by another program";

    /// Back the active tab with a file whose watcher is off, give it unsaved
    /// edits, and record `baseline` as the disk version the edits are based on.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `path`: The file backing the tab
    /// - `baseline`: The modification time of the version the buffer is based on
    ///
    /// ### Returns
    /// - `TabId`: The id of the prepared tab
    fn open_unwatched_dirty_tab(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        path: &Path,
        baseline: SystemTime,
    ) -> TabId {
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.stop_file_watcher();
                let tab = this.active_tab_entity(cx).expect("expected an active tab");
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                    editor_tab.location = TabLocation::Local(path.to_path_buf());
                    editor_tab.file_last_modified = Some(baseline);
                    editor_tab.content.update(cx, |state, cx| {
                        replace_editor_text(state, LOCAL_TEXT, window, cx);
                    });
                });
                tab.read(cx).id()
            })
        })
    }

    /// Save the active tab and let the background write complete.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    fn save_active_tab(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) {
        visual_cx.update(|window, cx| fulgur.update(cx, |this, cx| this.save_file(window, cx)));
        visual_cx.run_until_parked();
    }

    /// Save a dirty tab over a file rewritten by another program, so the save
    /// conflict dialog opens.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `path`: The file backing the tab
    ///
    /// ### Returns
    /// - `TabId`: The id of the tab whose save was refused
    fn raise_save_conflict(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        path: &Path,
    ) -> TabId {
        let baseline = write_with_mtime(path, "loaded", 1_000);
        let tab_id = open_unwatched_dirty_tab(fulgur, visual_cx, path, baseline);
        write_with_mtime(path, EXTERNAL_TEXT, 2_000);
        save_active_tab(fulgur, visual_cx);
        assert!(has_active_dialog(visual_cx));
        tab_id
    }

    /// Describe a tab's current buffer as the snapshot a UTF-8 save would write.
    ///
    /// ### Arguments
    /// - `this`: The window state owning the tab
    /// - `tab_id`: The tab to describe
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `SavedSnapshot`: The byte length, fingerprint and revision of the buffer
    fn saved_snapshot_of(this: &Fulgur, tab_id: TabId, cx: &App) -> SavedSnapshot {
        let tab = this.tab_entity_of(tab_id, cx).expect("expected the tab");
        let editor_tab = tab.read(cx).as_editor().expect("expected an editor tab");
        let prepared = SaveSnapshot::capture(editor_tab, cx).prepare(UTF_8);
        SavedSnapshot {
            byte_len: prepared.encoded.expect("UTF-8 encodes any text").len(),
            fingerprint: prepared.fingerprint,
        }
    }

    /// Read a tab's buffer text, dirty flag and disk baseline.
    ///
    /// ### Arguments
    /// - `fulgur`: The window state owning the tab
    /// - `visual_cx`: The visual test context of the window
    /// - `tab_id`: The tab to inspect
    ///
    /// ### Returns
    /// - `(String, bool, Option<SystemTime>)`: The buffer text, whether the tab is
    ///   modified, and the modification time its buffer is based on
    fn tab_state(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        tab_id: TabId,
    ) -> (String, bool, Option<SystemTime>) {
        visual_cx.update(|_, cx| {
            let tab = fulgur
                .read(cx)
                .tab_entity_of(tab_id, cx)
                .expect("tab should still exist");
            let editor_tab = tab.read(cx).as_editor().expect("expected an editor tab");
            (
                editor_tab.content.read(cx).text().to_string(),
                editor_tab.modified,
                editor_tab.file_last_modified,
            )
        })
    }

    // ========== save_file tests ==========

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
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
    #[gpui_kit::test]
    fn test_save_file_marks_tab_clean_only_after_write_completes(cx: &mut TestAppContext) {
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
                assert!(modified, "tab must remain dirty while the write is pending");
            });
        });
        visual_cx.run_until_parked();

        visual_cx.update(|_, cx| {
            let modified = fulgur
                .read(cx)
                .tabs
                .last()
                .and_then(|tab| tab.read(cx).as_editor())
                .is_none_or(|editor_tab| editor_tab.modified);
            assert!(
                !modified,
                "tab should become clean after the write succeeds"
            );
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_edit_during_local_save_stays_dirty(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("edited_during_save.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.tabs.last().expect("expected at least one tab").clone();
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected editor tab");
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.content.update(cx, |state, cx| {
                        state.set_value("dispatched", window, cx);
                    });
                });
                this.save_file(window, cx);
                tab.update(cx, |tab, cx| {
                    tab.as_editor_mut()
                        .expect("expected editor tab")
                        .content
                        .update(cx, |state, cx| {
                            state.set_value("edited later", window, cx);
                        });
                });
            });
        });
        visual_cx.run_until_parked();

        assert_eq!(
            std::fs::read_to_string(&path).expect("file should have been saved"),
            "dispatched"
        );
        visual_cx.update(|_, cx| {
            let editor_tab = fulgur.read(cx).tabs.last().expect("expected tab").read(cx);
            let editor_tab = editor_tab.as_editor().expect("expected editor tab");
            assert!(editor_tab.modified, "later edit must remain dirty");
            assert!(editor_tab.content_differs_from_original(cx));
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_same_length_edit_during_local_save_stays_dirty(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("same_length_edit.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.tabs.last().expect("expected at least one tab").clone();
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected editor tab");
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.content.update(cx, |state, cx| {
                        replace_editor_text(state, "aaaa", window, cx);
                    });
                });
            });
        });
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.save_file(window, cx);
                let tab = this.tabs.last().expect("expected at least one tab").clone();
                tab.update(cx, |tab, cx| {
                    tab.as_editor_mut()
                        .expect("expected editor tab")
                        .content
                        .update(cx, |state, cx| {
                            replace_editor_text(state, "bbbb", window, cx);
                        });
                });
            });
        });
        visual_cx.run_until_parked();

        assert_eq!(
            std::fs::read_to_string(&path).expect("file should have been saved"),
            "aaaa"
        );
        visual_cx.update(|_, cx| {
            let tab = fulgur.read(cx).tabs.last().expect("expected tab").read(cx);
            let editor_tab = tab.as_editor().expect("expected editor tab");
            assert!(
                editor_tab.modified,
                "an edit of the same length made during the write must remain dirty"
            );
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_save_then_close_waits_for_success(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("save_then_close.txt");
        let tab_id = visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.tabs.last().expect("expected tab").clone();
                let tab_id = tab.read(cx).id();
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected editor tab");
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.content.update(cx, |state, cx| {
                        state.set_value("saved before close", window, cx);
                    });
                });
                this.save_file(window, cx);
                let editor_tab = tab.read(cx);
                assert!(
                    editor_tab
                        .as_editor()
                        .expect("expected editor tab")
                        .content_differs_from_original(cx),
                    "pending save must remain recoverable in session state"
                );
                this.close_tab(tab_id, window, cx);
                assert!(this.tabs.iter().any(|tab| tab.read(cx).id() == tab_id));
                assert!(this.pending_save_tab_closes.contains(&tab_id));
                tab_id
            })
        });
        visual_cx.run_until_parked();

        visual_cx.update(|_, cx| {
            assert!(
                !fulgur
                    .read(cx)
                    .tabs
                    .iter()
                    .any(|tab| tab.read(cx).id() == tab_id)
            );
        });
        assert_eq!(
            std::fs::read_to_string(path).expect("file should have been saved"),
            "saved before close"
        );
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_failed_save_cancels_deferred_close_and_retains_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("missing-parent").join("missing.txt");
        let contents = "not persisted";

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab = this.tabs.last().expect("expected tab").clone();
                let tab_id = tab.read(cx).id();
                tab.update(cx, |tab, cx| {
                    let editor_tab = tab.as_editor_mut().expect("expected editor tab");
                    editor_tab.location = TabLocation::Local(path.clone());
                    editor_tab.content.update(cx, |state, cx| {
                        state.set_value(contents, window, cx);
                    });
                });
                this.inflight_saves.insert(tab_id, path.clone());
                this.close_tab(tab_id, window, cx);
                this.finish_local_save(
                    &SaveCompletion {
                        tab_id,
                        path: path.clone(),
                    },
                    Err(anyhow::anyhow!("delayed write failed")),
                    window,
                    cx,
                );

                let editor_tab = this
                    .tabs
                    .iter()
                    .map(|tab| tab.read(cx))
                    .find(|tab| tab.id() == tab_id)
                    .and_then(crate::fulgur::tab::Tab::as_editor)
                    .expect("failed save must retain the tab");
                assert!(editor_tab.content_differs_from_original(cx));
                assert!(!this.pending_save_tab_closes.contains(&tab_id));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
    fn test_quit_waits_for_pending_local_save(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = std::env::temp_dir().join("pending-quit-save.txt");

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tab_id = this.tabs.last().expect("expected tab").read(cx).id();
                this.inflight_saves.insert(tab_id, path.clone());

                this.quit(window, cx);

                assert_eq!(
                    this.pending_save_close_action,
                    Some(PendingSaveCloseAction::Quit)
                );
                assert!(this.tabs.iter().any(|tab| tab.read(cx).id() == tab_id));

                // This test exercises only the deferral boundary; do not leave
                // an artificial save pending after the assertion.
                this.inflight_saves.remove(&tab_id);
                this.pending_save_close_action = None;
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui_kit::test]
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
    #[gpui_kit::test]
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

    #[gpui_kit::test]
    fn test_save_refuses_file_changed_on_disk_while_unwatched(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("changed_on_disk.txt");

        let tab_id = raise_save_conflict(&fulgur, &mut visual_cx, &path);

        assert_eq!(
            std::fs::read_to_string(&path).expect("file should exist"),
            EXTERNAL_TEXT,
            "a save must not overwrite a change the watcher never reported"
        );
        let (text, modified, _) = tab_state(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(text, LOCAL_TEXT);
        assert!(modified, "a refused save must keep the edits dirty");
        visual_cx.update(|_, cx| {
            let this = fulgur.read(cx);
            assert!(this.inflight_saves.is_empty());
            assert!(this.file_watch_state.open_conflict_dialogs.contains(&path));
        });
    }

    #[gpui_kit::test]
    fn test_save_conflict_overwrite_writes_the_buffer(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("overwrite.txt");
        let tab_id = raise_save_conflict(&fulgur, &mut visual_cx, &path);

        click_dialog_button(&mut visual_cx, "save-conflict-overwrite");

        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            std::fs::read_to_string(&path).expect("file should exist"),
            LOCAL_TEXT
        );
        let (_, modified, baseline) = tab_state(&fulgur, &mut visual_cx, tab_id);
        assert!(!modified);
        assert_eq!(
            baseline,
            file_modified_system_time(&path),
            "the overwrite must become the new baseline, or the next save conflicts again"
        );
        visual_cx.update(|_, cx| {
            assert!(
                fulgur
                    .read(cx)
                    .file_watch_state
                    .open_conflict_dialogs
                    .is_empty()
            );
        });
    }

    #[gpui_kit::test]
    fn test_save_conflict_load_from_file_reloads_disk_version(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("load_from_file.txt");
        let tab_id = raise_save_conflict(&fulgur, &mut visual_cx, &path);

        click_dialog_button(&mut visual_cx, "save-conflict-load-from-file");

        assert!(!has_active_dialog(&mut visual_cx));
        let (text, modified, _) = tab_state(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(text, EXTERNAL_TEXT);
        assert!(!modified);
    }

    #[gpui_kit::test]
    fn test_save_conflict_cancel_keeps_both_versions(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("cancel.txt");
        let tab_id = raise_save_conflict(&fulgur, &mut visual_cx, &path);

        click_dialog_button(&mut visual_cx, "save-conflict-cancel");

        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            std::fs::read_to_string(&path).expect("file should exist"),
            EXTERNAL_TEXT
        );
        let (text, modified, _) = tab_state(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(text, LOCAL_TEXT);
        assert!(modified);
    }

    #[gpui_kit::test]
    fn test_save_over_unchanged_file_writes_without_prompt(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("unchanged.txt");
        let baseline = write_with_mtime(&path, "loaded", 1_000);
        open_unwatched_dirty_tab(&fulgur, &mut visual_cx, &path, baseline);

        save_active_tab(&fulgur, &mut visual_cx);

        assert!(!has_active_dialog(&mut visual_cx));
        assert_eq!(
            std::fs::read_to_string(&path).expect("file should exist"),
            LOCAL_TEXT
        );
    }

    #[gpui_kit::test]
    fn test_save_conflict_cancels_deferred_close(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("close_after_conflict.txt");
        let baseline = write_with_mtime(&path, "loaded", 1_000);
        let tab_id = open_unwatched_dirty_tab(&fulgur, &mut visual_cx, &path, baseline);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.inflight_saves.insert(tab_id, path.clone());
                this.close_tab(tab_id, window, cx);
                this.finish_local_save(
                    &SaveCompletion {
                        tab_id,
                        path: path.clone(),
                    },
                    Ok(LocalWriteOutcome::ChangedOnDisk),
                    window,
                    cx,
                );
                assert!(this.tab_entity_of(tab_id, cx).is_some());
                assert!(!this.pending_save_tab_closes.contains(&tab_id));
            });
        });
        visual_cx.run_until_parked();
        assert!(has_active_dialog(&mut visual_cx));
    }

    #[gpui_kit::test]
    fn test_write_landing_after_own_save_is_not_overwritten_next(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("write_after_rename.txt");
        let baseline = write_with_mtime(&path, "loaded", 1_000);
        let tab_id = open_unwatched_dirty_tab(&fulgur, &mut visual_cx, &path, baseline);
        let written =
            atomic_write_file(&path, LOCAL_TEXT.as_bytes()).expect("write should succeed");
        // Lands after the editor's rename, before its save completes.
        write_with_mtime(&path, EXTERNAL_TEXT, 3_000);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.inflight_saves.insert(tab_id, path.clone());
                let saved = saved_snapshot_of(this, tab_id, cx);
                this.finish_local_save(
                    &SaveCompletion {
                        tab_id,
                        path: path.clone(),
                    },
                    Ok(LocalWriteOutcome::Written(written.clone(), saved)),
                    window,
                    cx,
                );
            });
        });
        let (_, _, recorded) = tab_state(&fulgur, &mut visual_cx, tab_id);
        assert_eq!(
            recorded,
            written.modified().ok(),
            "the baseline must be the editor's own write, not the file found after it"
        );

        save_active_tab(&fulgur, &mut visual_cx);

        assert_eq!(
            std::fs::read_to_string(&path).expect("file should exist"),
            EXTERNAL_TEXT,
            "the next save must not silently overwrite the later external write"
        );
        assert!(has_active_dialog(&mut visual_cx));
    }
}
