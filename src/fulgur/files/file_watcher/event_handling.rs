use super::watcher::FileWatchEvent;
use crate::fulgur::Fulgur;
use crate::fulgur::editor_tab::TabLocation;
use crate::fulgur::tab::Tab;
use gpui::{Context, Window};
use std::path::PathBuf;
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
use std::time::Instant;

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
    fn mark_tab_deleted_externally(&mut self, path: &PathBuf, cx: &mut gpui::App) {
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

    /// Handle file watch events received from the file watcher
    ///
    /// ### Description
    /// - If the event is a modification, it shows a conflict dialog if the file is modified and the tab is active
    /// - If the event is a deletion whose path still exists, it is treated as an atomic-rename replacement (re-watch and reload)
    /// - If the event is a deletion whose path is gone, it shows a notification that the file was deleted
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
            FileWatchEvent::Modified(path) => {
                if self.should_suppress_file_watch_event(&path) {
                    return;
                }
                self.apply_external_modification(&path, window, cx);
            }
            FileWatchEvent::Deleted(path) => {
                if self.should_suppress_file_watch_event(&path) {
                    return;
                }
                // A "deleted" event whose path still exists on disk is an atomic save.
                if path.exists() {
                    // Re-register only on inode-based backends..
                    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
                    {
                        self.unwatch_file(&path);
                        self.watch_file(&path);
                        self.file_watch_state
                            .last_file_events
                            .insert(path.clone(), Instant::now());
                    }
                    self.apply_external_modification(&path, window, cx);
                    return;
                }
                self.mark_tab_deleted_externally(&path, cx);
                Self::show_notification_file_deleted(&path, window, cx);
            }
            FileWatchEvent::Renamed { from, to } => {
                if self.should_suppress_file_watch_event(&from) {
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
                                .unwrap_or("Untitled")
                                .to_string()
                                .into();
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
    use crate::fulgur::editor_tab::TabLocation;
    use crate::fulgur::files::file_watcher::FileWatchEvent;
    use crate::fulgur::test_support::{setup_fulgur_with_root as setup_fulgur, temp_test_path};
    use gpui::TestAppContext;
    use std::time::{Duration, Instant};
    use tempfile::TempDir;

    #[gpui::test]
    fn test_handle_file_watch_event_modified_reloads_unmodified_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("modified_reload_test.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
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
                    this.file_watch_state.last_file_events.contains_key(&path),
                    "modified event should update debounce map"
                );
            });
        });
        // Reloading runs on the background executor, so wait for it to apply.
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "content-from-disk");
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_modified_active_tab_does_not_queue_conflict(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_conflict_active.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.modified = true;
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local-edits", window, cx);
                            });
                        }
                    });
                this.active_tab_id = this.tabs.first().map(|t| t.read(cx).id());
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&path),
                    "active-tab conflict should prompt immediately, not queue"
                );
            });
        });
    }

    #[gpui::test]
    fn test_repeated_external_modifications_show_a_single_conflict_dialog(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_conflict_repeated.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs
                    .first()
                    .expect("expected at least one tab")
                    .clone()
                    .update(cx, |tab, cx| {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.modified = true;
                            editor_tab.content.update(cx, |input_state, cx| {
                                input_state.set_value("local-edits", window, cx);
                            });
                        }
                    });
                this.active_tab_id = this.tabs.first().map(|t| t.read(cx).id());

                for _ in 0..3 {
                    // Backdate the debounce marker so every iteration counts as
                    // a genuinely separate external write rather than a burst.
                    let stale = Instant::now()
                        .checked_sub(Duration::from_secs(1))
                        .expect("instant subtraction should not underflow");
                    this.file_watch_state
                        .last_file_events
                        .insert(path.clone(), stale);
                    this.handle_file_watch_event(
                        FileWatchEvent::Modified(path.clone()),
                        window,
                        cx,
                    );
                }

                assert_eq!(
                    this.file_watch_state.open_conflict_dialogs.len(),
                    1,
                    "repeated external writes must not stack conflict dialogs"
                );
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_modified_inactive_tab_defers_until_activation(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let deferred_path = temp_test_path("fulgur_conflict_inactive.txt");
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
                this.handle_file_watch_event(
                    FileWatchEvent::Modified(deferred_path.clone()),
                    window,
                    cx,
                );
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

    #[gpui::test]
    fn test_handle_file_watch_event_deleted_keeps_editor_state(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_deleted_branch.txt");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.tabs.first().expect("expected at least one tab").clone().update(cx, |tab, cx| {
                    if let Some(editor_tab) = tab.as_editor_mut() {
                        editor_tab.location = TabLocation::Local(path.clone());
                        editor_tab.content.update(cx, |input_state, cx| {
                            input_state.set_value("current-content", window, cx);
                        });
                        editor_tab.set_original_content_from_str("current-content");
                        editor_tab.title = "deleted_branch.txt".into();
                    }
                });
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
                let (current_path, current_title, current_content, current_modified) = this
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
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_deleted_existing_path_reloads(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("atomic_rename_reload.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
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
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
            });
        });
        // Reloading runs on the background executor, so wait for it to apply.
        visual_cx.run_until_parked();

        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(
                    content, "content-from-disk",
                    "a delete whose path still exists is an atomic-rename replacement and should reload"
                );
            });
        });
    }

    #[gpui::test]
    fn test_handle_file_watch_event_renamed_updates_path_and_title(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let from = temp_test_path("fulgur_rename_from.rs");
        let to = temp_test_path("fulgur_rename_to.rs");
        visual_cx.update(|window, cx| {
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
                // Seed stale bookkeeping (older than the 500 ms suppression
                // window): a genuine external rename happens long after any
                // prior save, so it must still be processed and prune these.
                let stale = Instant::now()
                    .checked_sub(Duration::from_secs(1))
                    .expect("instant subtraction should not underflow");
                this.file_watch_state
                    .last_file_events
                    .insert(from.clone(), stale);
                this.file_watch_state
                    .last_file_saves
                    .insert(from.clone(), stale);
                this.file_watch_state
                    .pending_conflicts
                    .insert(from.clone(), 0);
                this.handle_file_watch_event(
                    FileWatchEvent::Renamed {
                        from: from.clone(),
                        to: to.clone(),
                    },
                    window,
                    cx,
                );
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
                    !this.file_watch_state.last_file_events.contains_key(&from),
                    "rename should prune old-path debounce bookkeeping"
                );
                assert!(
                    !this.file_watch_state.last_file_saves.contains_key(&from),
                    "rename should prune old-path save bookkeeping"
                );
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&from),
                    "rename should prune old-path deferred conflict bookkeeping"
                );
            });
        });
    }
}
