use super::watcher::FileWatchEvent;
use crate::fulgur::Fulgur;
use crate::fulgur::editor_tab::TabLocation;
use crate::fulgur::tab::Tab;
use crate::fulgur::ui::components_utils::UNTITLED;
use gpui_kit::{Context, Window};
use std::path::PathBuf;

impl Fulgur {
    /// Apply an external-modification event to the tab backing a path.
    ///
    /// ### Arguments
    /// - `path`: The path of the externally modified file
    /// - `window`: The window context
    /// - `cx`: The application context
    fn apply_external_modification(
        &mut self,
        path: &PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_index) = self.find_tab_by_path(path, cx)
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index).map(|t| t.read(cx))
        {
            if editor_tab.log_view {
                return;
            }
            if editor_tab.modified {
                let tab_id = editor_tab.id;
                let is_active = self.active_tab_index(cx) == Some(tab_index);

                if is_active {
                    self.show_file_conflict_dialog(path, tab_id, window, cx);
                } else {
                    self.file_watch_state
                        .pending_conflicts
                        .insert(path.clone(), tab_index);
                }
            } else {
                self.reload_tab_from_disk(tab_index, window, cx);
                Self::show_notification_file_reloaded(path, window, cx);
            }
        }
    }

    /// Mark the tab backing an externally deleted file as modified.
    ///
    /// ### Arguments
    /// - `path`: The path of the file that was deleted externally
    /// - `cx`: The application context
    fn mark_tab_deleted_externally(&mut self, path: &PathBuf, cx: &mut gpui_kit::App) {
        self.file_watch_state.pending_conflicts.remove(path);
        if let Some(tab_entity) = self
            .find_tab_by_path(path, cx)
            .and_then(|tab_index| self.tabs.get(tab_index).cloned())
        {
            tab_entity.update(cx, |tab, cx| {
                if let Some(editor_tab) = tab.as_editor_mut() {
                    editor_tab.modified = true;
                    cx.notify();
                }
            });
        }
    }

    /// Re-check a watched path once its watch events have gone quiet and apply
    /// the change the file actually went through
    ///
    /// ### Description
    /// - If a save of the editor is writing the path, nothing happens: its completion records the new state
    /// - If the file's size and modification time match the known state (an echo of the editor's own save, or a metadata-only change), nothing happens
    /// - If the file exists, it is an external modification (an atomic-rename replacement included): the tab is reloaded or a conflict is raised
    /// - If the file is gone, the tab is marked modified and a deletion notification is shown
    ///
    /// ### Arguments
    /// - `path`: The path whose scheduled check fired
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn run_file_change_check(
        &mut self,
        path: &PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.file_watch_state.scheduled_checks.remove(path);
        if self.is_saving_path(path) {
            return;
        }
        if let Some(watcher) = &mut self.file_watch_state.file_watcher
            && !watcher.acknowledge_disk_change(path)
        {
            return;
        }
        if path.exists() {
            // Inode-based backends lose the watch when the file is replaced.
            #[cfg(not(any(target_os = "macos", target_os = "windows")))]
            {
                if let Some(watcher) = &mut self.file_watch_state.file_watcher
                    && let Err(e) = watcher.rewatch_file(path)
                {
                    log::warn!("{e}");
                }
            }
            self.apply_external_modification(path, window, cx);
        } else {
            self.mark_tab_deleted_externally(path, cx);
            Self::show_notification_file_deleted(path, window, cx);
        }
    }

    /// Handle file watch events received from the file watcher
    ///
    /// ### Description
    /// - If the event is a modification or a deletion, the path is re-checked once its events go quiet (see `run_file_change_check`)
    /// - If the event is a rename, it shows a notification that the file was renamed
    /// - If the event is an error, it logs the error
    ///
    /// ### Arguments
    /// - `event`: The file watch event to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn handle_file_watch_event(
        &mut self,
        event: FileWatchEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            FileWatchEvent::Modified(path) | FileWatchEvent::Deleted(path) => {
                self.schedule_file_change_check(path, window, cx);
            }
            FileWatchEvent::Renamed { from, to } => {
                if self.is_saving_path(&from) {
                    return;
                }
                if let Some(tab_entity) = self
                    .find_tab_by_path(&from, cx)
                    .and_then(|tab_index| self.tabs.get(tab_index).cloned())
                {
                    self.unwatch_file(&from);
                    self.watch_file(&to);
                    tab_entity.update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(to.clone());
                            editor_tab.title = to
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or(UNTITLED)
                                .to_string()
                                .into();
                            editor_tab.update_language(cx);
                            cx.notify();
                        }
                    });
                    Self::show_notification_file_renamed(&from, &to, window, cx);
                }
            }
            FileWatchEvent::Error(msg) => {
                log::error!("File watcher error: {msg}");
            }
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::Fulgur;
    use crate::fulgur::editor_tab::TabLocation;
    use crate::fulgur::files::file_watcher::FileWatchEvent;
    use crate::fulgur::files::file_watcher::test_helpers::{
        settle_file_checks, setup_fulgur, temp_test_path,
    };
    use crate::fulgur::languages::supported_languages::SupportedLanguage;
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use std::path::{Path, PathBuf};
    use tempfile::TempDir;

    /// Create a file on disk so a scheduled check sees it as modified rather than deleted.
    ///
    /// ### Arguments
    /// - `dir`: The temporary directory holding the file
    /// - `file_name`: The file name inside `dir`
    ///
    /// ### Returns
    /// - `PathBuf`: The path of the created file
    fn existing_file(dir: &TempDir, file_name: &str) -> PathBuf {
        let path = dir.path().join(file_name);
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        path
    }

    /// Point the first tab at a path with locally modified, unsaved content.
    ///
    /// ### Arguments
    /// - `fulgur`: The Fulgur entity under test
    /// - `visual_cx`: The visual test context driving the window
    /// - `path`: The file backing the tab
    fn open_dirty_active_tab(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        path: &Path,
    ) {
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.to_path_buf());
                            editor_tab.modified = true;
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local-edits", window, cx);
                            });
                        }
                    });
                this.active_tab_id = this.tabs.first().map(|t| t.read(cx).id());
            });
        });
    }

    /// Deliver a watch event to the window.
    ///
    /// ### Arguments
    /// - `fulgur`: The Fulgur entity under test
    /// - `visual_cx`: The visual test context driving the window
    /// - `event`: The file watch event to deliver
    fn deliver(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext, event: FileWatchEvent) {
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.handle_file_watch_event(event, window, cx);
            });
        });
    }

    /// Read the first tab's buffer text.
    ///
    /// ### Arguments
    /// - `fulgur`: The Fulgur entity under test
    /// - `visual_cx`: The visual test context driving the window
    ///
    /// ### Returns
    /// - `String`: The buffer text, empty if the first tab is not an editor
    fn first_tab_text(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) -> String {
        visual_cx.update(|_window, cx| {
            fulgur
                .read(cx)
                .tabs
                .first()
                .and_then(|t| t.read(cx).as_editor())
                .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                .unwrap_or_default()
        })
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_modified_reloads_unmodified_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = existing_file(&dir, "modified_reload_test.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("stale-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("stale-content");
                            editor_tab.modified = false;
                        }
                    });
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                assert!(
                    this.file_watch_state.scheduled_checks.contains_key(&path),
                    "modified event should schedule a check of the path"
                );
            });
        });
        assert_eq!(
            first_tab_text(&fulgur, &mut visual_cx),
            "stale-content",
            "the reload must wait for the events to go quiet"
        );
        settle_file_checks(&mut visual_cx);
        assert_eq!(first_tab_text(&fulgur, &mut visual_cx), "content-from-disk");
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_modified_active_tab_does_not_queue_conflict(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = existing_file(&dir, "conflict_active.txt");
        open_dirty_active_tab(&fulgur, &mut visual_cx, &path);
        deliver(
            &fulgur,
            &mut visual_cx,
            FileWatchEvent::Modified(path.clone()),
        );
        settle_file_checks(&mut visual_cx);
        visual_cx.update(|_window, cx| {
            let state = &fulgur.read(cx).file_watch_state;
            assert!(
                !state.pending_conflicts.contains_key(&path),
                "active-tab conflict should prompt immediately, not queue"
            );
            assert!(state.open_conflict_dialogs.contains(&path));
        });
    }

    #[gpui_kit::test]
    fn test_repeated_external_modifications_show_a_single_conflict_dialog(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = existing_file(&dir, "conflict_repeated.txt");
        open_dirty_active_tab(&fulgur, &mut visual_cx, &path);
        for _ in 0..3 {
            // Let each event settle so every iteration counts as a genuinely
            // separate external write rather than a burst.
            deliver(
                &fulgur,
                &mut visual_cx,
                FileWatchEvent::Modified(path.clone()),
            );
            settle_file_checks(&mut visual_cx);
        }
        visual_cx.update(|_window, cx| {
            assert_eq!(
                fulgur.read(cx).file_watch_state.open_conflict_dialogs.len(),
                1,
                "repeated external writes must not stack conflict dialogs"
            );
        });
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_modified_inactive_tab_defers_until_activation(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let deferred_path = existing_file(&dir, "conflict_inactive.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(deferred_path.clone());
                            editor_tab.modified = true;
                        }
                    });
                this.set_active_tab(1, window, cx);
            });
        });
        deliver(
            &fulgur,
            &mut visual_cx,
            FileWatchEvent::Modified(deferred_path.clone()),
        );
        settle_file_checks(&mut visual_cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert_eq!(
                    this.file_watch_state.pending_conflicts.get(&deferred_path),
                    Some(&0),
                    "inactive modified tab should queue deferred conflict"
                );
                this.set_active_tab(0, window, cx);
                assert!(
                    !this
                        .file_watch_state
                        .pending_conflicts
                        .contains_key(&deferred_path),
                    "deferred conflict should be consumed when tab is activated"
                );
            });
        });
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_deleted_keeps_editor_state(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_deleted_branch.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("current-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("current-content");
                            editor_tab.title = "deleted_branch.txt".into();
                        }
                    });
            });
        });
        deliver(
            &fulgur,
            &mut visual_cx,
            FileWatchEvent::Deleted(path.clone()),
        );
        settle_file_checks(&mut visual_cx);
        visual_cx.update(|_window, cx| {
            let (current_path, current_title, current_content, current_modified) = fulgur
                .read(cx)
                .tabs
                .first()
                .and_then(|t| t.read(cx).as_editor())
                .map(|editor_tab| {
                    (
                        editor_tab.file_path().cloned(),
                        editor_tab.title.to_string(),
                        editor_tab.content.read(cx).text().to_string(),
                        editor_tab.modified,
                    )
                })
                .expect("expected active editor tab");
            assert_eq!(current_path, Some(path));
            assert_eq!(current_title, "deleted_branch.txt");
            assert_eq!(current_content, "current-content");
            assert!(
                current_modified,
                "a genuine external deletion should mark the tab modified so closing prompts to save"
            );
        });
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_deleted_existing_path_reloads(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = existing_file(&dir, "atomic_rename_reload.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("stale-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("stale-content");
                            editor_tab.modified = false;
                        }
                    });
            });
        });
        deliver(
            &fulgur,
            &mut visual_cx,
            FileWatchEvent::Deleted(path.clone()),
        );
        settle_file_checks(&mut visual_cx);
        assert_eq!(
            first_tab_text(&fulgur, &mut visual_cx),
            "content-from-disk",
            "a delete whose path still exists is an atomic-rename replacement and should reload"
        );
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_renamed_updates_path_and_title(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let from = temp_test_path("fulgur_rename_from.rs");
        let to = temp_test_path("fulgur_rename_to.rs");
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(from.clone());
                            editor_tab.title = "fulgur_rename_from.rs".into();
                        }
                    });
                this.file_watch_state
                    .pending_conflicts
                    .insert(from.clone(), 0);
            });
        });
        // A check still scheduled for the old path must not outlive the rename.
        deliver(
            &fulgur,
            &mut visual_cx,
            FileWatchEvent::Modified(from.clone()),
        );
        deliver(
            &fulgur,
            &mut visual_cx,
            FileWatchEvent::Renamed {
                from: from.clone(),
                to: to.clone(),
            },
        );
        visual_cx.update(|_window, cx| {
            let this = fulgur.read(cx);
            let (current_path, current_title) = this
                .tabs
                .first()
                .and_then(|t| t.read(cx).as_editor())
                .map(|editor_tab| {
                    (
                        editor_tab.file_path().cloned(),
                        editor_tab.title.to_string(),
                    )
                })
                .expect("expected active editor tab");
            assert_eq!(current_path, Some(to));
            assert_eq!(current_title, "fulgur_rename_to.rs");
            assert!(
                !this.file_watch_state.scheduled_checks.contains_key(&from),
                "rename should cancel the old path's scheduled check"
            );
            assert!(
                !this.file_watch_state.pending_conflicts.contains_key(&from),
                "rename should prune old-path deferred conflict bookkeeping"
            );
        });
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_renamed_refreshes_language(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let from = temp_test_path("fulgur_rename_language_from.txt");
        let to = temp_test_path("fulgur_rename_language_to.rs");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, _cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(from.clone());
                            editor_tab.title = "fulgur_rename_language_from.txt".into();
                            editor_tab.language = SupportedLanguage::Plain;
                        }
                    });
                this.handle_file_watch_event(
                    FileWatchEvent::Renamed {
                        from: from.clone(),
                        to: to.clone(),
                    },
                    window,
                    cx,
                );
                let language = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.language)
                    .expect("expected active editor tab");
                assert_eq!(
                    language,
                    SupportedLanguage::Rust,
                    "rename should re-detect the language from the new extension"
                );
            });
        });
    }
}
