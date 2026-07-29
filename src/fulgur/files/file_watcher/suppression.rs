use crate::fulgur::Fulgur;
use std::path::PathBuf;
use std::time::{Duration, Instant};

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

    /// Determine whether a watch event for a path should be ignored as a
    /// self-save echo (completed or still in flight) or a duplicate within the
    /// debounce window.
    ///
    /// ### Arguments
    /// - `path`: The file path the event refers to
    ///
    /// ### Returns
    /// - `true`: The event is a self-save echo or duplicate and should be ignored
    /// - `false`: The event is new and the debounce timestamp has been recorded
    pub(super) fn should_suppress_file_watch_event(&mut self, path: &PathBuf) -> bool {
        let now = Instant::now();
        if let Some(&last_time) = self.file_watch_state.last_file_events.get(path)
            && now.duration_since(last_time) < Duration::from_millis(500)
        {
            return true;
        }
        if let Some(&save_time) = self.file_watch_state.last_file_saves.get(path)
            && now.duration_since(save_time) < Duration::from_millis(500)
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
        self.file_watch_state
            .last_file_events
            .insert(path.clone(), now);
        false
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::editor_tab::TabLocation;
    use crate::fulgur::files::file_watcher::FileWatchEvent;
    use crate::fulgur::files::file_watcher::test_helpers::setup_fulgur;
    use gpui::TestAppContext;
    use std::time::Instant;
    use tempfile::TempDir;

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
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

    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
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
