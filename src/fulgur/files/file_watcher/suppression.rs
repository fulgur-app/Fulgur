use super::state::ScheduledFileCheck;
use crate::fulgur::Fulgur;
use gpui_kit::{Context, Window};
use std::path::{Path, PathBuf};
use std::time::Duration;

/// Quiet period a path's watch events must observe before the file is re-checked.
pub(super) const FILE_WATCH_DEBOUNCE: Duration = Duration::from_millis(500);

/// Longest a burst of watch events can postpone the re-check of a path, so a
/// file that is written continuously is still picked up periodically.
pub(super) const FILE_WATCH_MAX_DELAY: Duration = Duration::from_secs(2);

impl Fulgur {
    /// Remove file-watcher bookkeeping entries for one file path, cancelling its scheduled check.
    ///
    /// ### Arguments
    /// - `path`: File path to remove from the scheduled-check and conflict maps
    pub(super) fn prune_file_watch_bookkeeping_for_path(&mut self, path: &PathBuf) {
        self.file_watch_state.scheduled_checks.remove(path);
        self.file_watch_state.pending_conflicts.remove(path);
    }

    /// Clear all file-watcher bookkeeping maps, cancelling every scheduled check.
    pub(super) fn clear_file_watch_bookkeeping(&mut self) {
        self.file_watch_state.scheduled_checks.clear();
        self.file_watch_state.pending_conflicts.clear();
    }

    /// Record that the editor just wrote a local file itself, so the watch
    /// events echoing that write are not mistaken for an external change
    ///
    /// ### Arguments
    /// - `path`: The file that was written
    pub(crate) fn record_local_file_save(&mut self, path: &Path) {
        if let Some(watcher) = &mut self.file_watch_state.file_watcher {
            watcher.refresh_disk_signature(path);
        }
    }

    /// Check whether a background save of the editor is currently writing a path
    ///
    /// ### Arguments
    /// - `path`: The file path to look up
    ///
    /// ### Returns
    /// - `true`: A save to this path is in flight
    /// - `false`: No save to this path is in flight
    pub(super) fn is_saving_path(&self, path: &Path) -> bool {
        self.inflight_saves
            .values()
            .any(|save_path| save_path == path)
    }

    /// Schedule a re-check of a path once its watch events go quiet
    ///
    /// ### Arguments
    /// - `path`: The file path the event refers to
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn schedule_file_change_check(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.is_saving_path(&path) {
            return;
        }
        let now = cx.background_executor().now();
        let first_event_at = match self.file_watch_state.scheduled_checks.get(&path) {
            Some(check)
                if now.duration_since(check.first_event_at) + FILE_WATCH_DEBOUNCE
                    >= FILE_WATCH_MAX_DELAY =>
            {
                return;
            }
            Some(check) => check.first_event_at,
            None => now,
        };
        let check_path = path.clone();
        let task = cx.spawn_in(window, async move |view, cx| {
            cx.background_executor().timer(FILE_WATCH_DEBOUNCE).await;
            view.update_in(cx, |this, window, cx| {
                this.run_file_change_check(&check_path, window, cx);
            })
            .ok();
        });
        self.file_watch_state.scheduled_checks.insert(
            path,
            ScheduledFileCheck {
                first_event_at,
                task,
            },
        );
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{FILE_WATCH_DEBOUNCE, FILE_WATCH_MAX_DELAY};
    use crate::fulgur::Fulgur;
    use crate::fulgur::editor_tab::TabLocation;
    use crate::fulgur::files::file_watcher::test_helpers::{
        advance_file_watch_clock, settle_file_checks, setup_fulgur,
    };
    use crate::fulgur::files::file_watcher::{FileWatchEvent, FileWatcher};
    use gpui_kit::{Context, Entity, TestAppContext, VisualTestContext, Window};
    use std::path::Path;
    use std::time::Duration;
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

    /// Deliver a `Modified` watch event for a path to the window.
    ///
    /// ### Arguments
    /// - `fulgur`: The Fulgur entity under test
    /// - `visual_cx`: The visual test context driving the window
    /// - `path`: The path the event refers to
    fn deliver_modified(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext, path: &Path) {
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.handle_file_watch_event(
                    FileWatchEvent::Modified(path.to_path_buf()),
                    window,
                    cx,
                );
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
            });
        });
        // Simulate echoes (e.g. macOS setting xattrs) arriving long after the save.
        advance_file_watch_clock(&mut visual_cx, Duration::from_secs(5));
        deliver_modified(&fulgur, &mut visual_cx, &path);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
            });
        });
        settle_file_checks(&mut visual_cx);
        assert_eq!(
            first_tab_text(&fulgur, &mut visual_cx),
            "buffer-text",
            "an echo of the editor's own save must not reload, however late it arrives"
        );
    }

    #[gpui_kit::test]
    fn test_external_change_right_after_own_save_reloads(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("external_after_save.txt");
        std::fs::write(&path, "before save").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                open_watched_unmodified_tab(this, &path, "buffer-text", window, cx);
                this.record_local_file_save(&path);
            });
        });
        std::fs::write(&path, "rewritten by another program").expect("failed to rewrite test file");
        deliver_modified(&fulgur, &mut visual_cx, &path);
        settle_file_checks(&mut visual_cx);
        assert_eq!(
            first_tab_text(&fulgur, &mut visual_cx),
            "rewritten by another program",
            "an external write right after the editor's own save must not be hidden as an echo"
        );
    }

    #[gpui_kit::test]
    fn test_truncate_then_rewrite_reloads_only_the_final_content(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("truncate_rewrite.txt");
        std::fs::write(&path, "before").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                open_watched_unmodified_tab(this, &path, "buffer-text", window, cx);
            });
        });
        let half_window = FILE_WATCH_DEBOUNCE / 2;

        std::fs::write(&path, "").expect("failed to truncate test file");
        deliver_modified(&fulgur, &mut visual_cx, &path);
        advance_file_watch_clock(&mut visual_cx, half_window);
        std::fs::write(&path, "final content").expect("failed to rewrite test file");
        deliver_modified(&fulgur, &mut visual_cx, &path);
        advance_file_watch_clock(&mut visual_cx, half_window + Duration::from_millis(10));
        assert_eq!(
            first_tab_text(&fulgur, &mut visual_cx),
            "buffer-text",
            "each event must restart the quiet period instead of acting on the first one"
        );

        settle_file_checks(&mut visual_cx);
        assert_eq!(first_tab_text(&fulgur, &mut visual_cx), "final content");
        visual_cx.update(|_window, cx| {
            let fulgur = fulgur.read(cx);
            assert!(fulgur.file_watch_state.scheduled_checks.is_empty());
            assert!(
                fulgur
                    .tabs
                    .first()
                    .and_then(|t| t.read(cx).as_editor())
                    .is_some_and(|editor_tab| !editor_tab.modified)
            );
        });
    }

    #[gpui_kit::test]
    fn test_continuous_writes_are_checked_within_the_max_delay(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("continuous_writes.txt");
        std::fs::write(&path, "before").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                open_watched_unmodified_tab(this, &path, "buffer-text", window, cx);
            });
        });
        let interval = FILE_WATCH_DEBOUNCE * 4 / 5;
        let mut elapsed = Duration::ZERO;
        let mut write_index = 0;
        while elapsed < FILE_WATCH_MAX_DELAY {
            std::fs::write(&path, format!("write {write_index}"))
                .expect("failed to rewrite test file");
            deliver_modified(&fulgur, &mut visual_cx, &path);
            advance_file_watch_clock(&mut visual_cx, interval);
            elapsed += interval;
            write_index += 1;
        }
        assert_ne!(
            first_tab_text(&fulgur, &mut visual_cx),
            "buffer-text",
            "a file that never stops changing must still be reloaded once the max delay passes"
        );
    }

    #[gpui_kit::test]
    fn test_event_during_inflight_save_is_ignored(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("inflight_save.txt");
        std::fs::write(&path, "content-from-disk").expect("failed to write test file");
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                open_watched_unmodified_tab(this, &path, "buffer-text", window, cx);
                let tab_id = this
                    .tabs
                    .first()
                    .expect("expected at least one tab")
                    .read(cx)
                    .id();
                this.inflight_saves.insert(tab_id, path.clone());
                this.handle_file_watch_event(FileWatchEvent::Deleted(path.clone()), window, cx);
                assert!(
                    this.file_watch_state.scheduled_checks.is_empty(),
                    "an event echoing a save still in flight must not schedule a check"
                );
            });
        });
        settle_file_checks(&mut visual_cx);
        assert_eq!(first_tab_text(&fulgur, &mut visual_cx), "buffer-text");
    }
}
