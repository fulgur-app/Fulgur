use crate::fulgur::Fulgur;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

/// Window during which a watch event for a path is treated as a duplicate of a
/// previous event or as an echo of the editor's own save.
const FILE_WATCH_DEBOUNCE: Duration = Duration::from_millis(500);

impl Fulgur {
    /// Remove file-watcher bookkeeping entries for one file path.
    ///
    /// ### Arguments
    /// - `path`: File path to remove from debounce/save/conflict maps
    pub(super) fn prune_file_watch_bookkeeping_for_path(&mut self, path: &PathBuf) {
        self.file_watch_state.last_file_events.remove(path);
        self.file_watch_state.last_file_saves.remove(path);
        self.file_watch_state.pending_conflicts.remove(path);
    }

    /// Clear all file-watcher bookkeeping maps.
    pub(super) fn clear_file_watch_bookkeeping(&mut self) {
        self.file_watch_state.last_file_events.clear();
        self.file_watch_state.last_file_saves.clear();
        self.file_watch_state.pending_conflicts.clear();
    }

    /// Record that the editor just wrote a local file itself
    ///
    /// ### Arguments
    /// - `path`: The file that was written
    pub(crate) fn record_local_file_save(&mut self, path: &Path) {
        self.file_watch_state
            .last_file_saves
            .insert(path.to_path_buf(), Instant::now());
        if let Some(watcher) = &mut self.file_watch_state.file_watcher {
            watcher.refresh_disk_signature(path);
        }
    }

    /// Determine whether a watch event for a path should be ignored as a
    /// self-save echo (completed or still in flight), a duplicate within the
    /// debounce window, or an event that left the file's size and modification
    /// time unchanged.
    ///
    /// ### Arguments
    /// - `path`: The file path the event refers to
    ///
    /// ### Returns
    /// - `true`: The event is an echo, a duplicate, or not a content change and should be ignored
    /// - `false`: The event is new and the debounce timestamp has been recorded
    pub(super) fn should_suppress_file_watch_event(&mut self, path: &PathBuf) -> bool {
        let now = Instant::now();
        if let Some(&last_time) = self.file_watch_state.last_file_events.get(path)
            && now.duration_since(last_time) < FILE_WATCH_DEBOUNCE
        {
            return true;
        }
        if let Some(&save_time) = self.file_watch_state.last_file_saves.get(path)
            && now.duration_since(save_time) < FILE_WATCH_DEBOUNCE
        {
            return true;
        }
        // A background save may have already renamed the file into place while
        // its completion handler (which records `last_file_saves`) has not run
        // yet; treat events for such paths as self-save echoes too.
        if self
            .inflight_saves
            .values()
            .any(|save_path| save_path == path)
        {
            return true;
        }
        // Late echoes of the editor's own saves and metadata-only changes (such
        // as macOS adding extended attributes) leave the known signature intact.
        if let Some(watcher) = &mut self.file_watch_state.file_watcher
            && !watcher.acknowledge_disk_change(path)
        {
            return true;
        }
        self.file_watch_state
            .last_file_events
            .insert(path.clone(), now);
        false
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::Fulgur;
    use crate::fulgur::editor_tab::TabLocation;
    use crate::fulgur::files::file_watcher::test_helpers::setup_fulgur;
    use crate::fulgur::files::file_watcher::{FileWatchEvent, FileWatcher};
    use gpui_kit::{Context, TestAppContext, Window};
    use std::path::Path;
    use std::time::Instant;
    use tempfile::TempDir;

    /// Point the first tab at a watched file while keeping distinct buffer text,
    /// so a reload from disk is observable.
    ///
    /// ### Arguments
    /// - `this`: The Fulgur instance under test
    /// - `path`: The file to back the tab and watch
    /// - `buffer_text`: Unmodified text placed in the tab's buffer
    /// - `window`: The window context
    /// - `cx`: The application context
    fn open_watched_unmodified_tab(
        this: &mut Fulgur,
        path: &Path,
        buffer_text: &str,
        window: &mut Window,
        cx: &mut Context<Fulgur>,
    ) {
        this.tabs
            .first()
            .expect("expected at least one tab")
            .clone()
            .update(cx, |tab, cx| {
                if let Some(editor_tab) = tab.as_editor_mut() {
                    editor_tab.location = TabLocation::Local(path.to_path_buf());
                    editor_tab.content.update(cx, |input_state, cx| {
                        input_state.set_value(buffer_text, window, cx);
                    });
                    editor_tab.set_original_content_from_str(buffer_text);
                    editor_tab.modified = false;
                }
            });
        let (mut watcher, _events) = FileWatcher::new();
        watcher.track_without_os_watch(path);
        this.file_watch_state.file_watcher = Some(watcher);
    }

    /// Read the first tab's buffer text.
    ///
    /// ### Arguments
    /// - `this`: The Fulgur instance under test
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `String`: The buffer text, empty if the first tab is not an editor
    fn first_tab_text(this: &Fulgur, cx: &Context<Fulgur>) -> String {
        this.tabs
            .first()
            .and_then(|t| t.read(cx).as_editor())
            .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
            .unwrap_or_default()
    }

    #[gpui_kit::test]
    fn test_late_echo_of_own_save_does_not_reload(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("late_save_echo.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                open_watched_unmodified_tab(this, &path, "buffer-text", window, cx);
                std::fs::write(&path, "written by the editor").expect("failed to save test file");
                this.record_local_file_save(&path);
                // Simulate an echo (e.g. macOS setting xattrs) arriving after the window.
                this.file_watch_state.last_file_saves.remove(&path);
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
            });
        });
        // Reloading runs on the background executor, so let any reload apply.
        visual_cx.run_until_parked();
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                assert_eq!(
                    first_tab_text(this, cx),
                    "buffer-text",
                    "an echo of the editor's own save must not reload, however late it arrives"
                );
            });
        });
    }

    #[gpui_kit::test]
    fn test_external_change_after_own_save_still_reloads(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("external_after_save.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                open_watched_unmodified_tab(this, &path, "buffer-text", window, cx);
                this.record_local_file_save(&path);
                this.file_watch_state.last_file_saves.remove(&path);
                std::fs::write(&path, "rewritten by another program")
                    .expect("failed to rewrite test file");
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
            });
        });
        visual_cx.run_until_parked();
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                assert_eq!(first_tab_text(this, cx), "rewritten by another program");
            });
        });
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_modified_is_debounced(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("modified_debounce_test.txt");
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
                                input_state.set_value("local-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("local-content");
                            editor_tab.modified = false;
                        }
                    });
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.handle_file_watch_event(FileWatchEvent::Modified(path.clone()), window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(content, "local-content");
            });
        });
    }

    #[gpui_kit::test]
    fn test_handle_file_watch_event_deleted_is_suppressed_after_self_save(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("self_save_suppressed.txt");
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
                                input_state.set_value("local-content", window, cx);
                            });
                            editor_tab.set_original_content_from_str("local-content");
                            editor_tab.modified = false;
                        }
                    });
                this.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), Instant::now());
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
                let content = this
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .map(|editor_tab| editor_tab.content.read(cx).text().to_string())
                    .unwrap_or_default();
                assert_eq!(
                    content, "local-content",
                    "a delete echoing the user's own atomic save must be suppressed, not reloaded"
                );
            });
        });
    }
}
