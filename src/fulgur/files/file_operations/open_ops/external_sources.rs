use crate::fulgur::{Fulgur, window_manager};
use gpui::{Context, ExternalPaths, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::{collections::HashSet, path::PathBuf};

impl Fulgur {
    /// Handle opening a file from the command line (double-click or "Open with")
    ///
    /// ### Behavior
    /// - If a tab exists for the file in this window: focus the tab and prompt when unsaved changes exist
    /// - If a tab exists in another window: show notification
    /// - If no tab exists: open a new tab and focus it
    ///
    /// ### Arguments
    /// - `window`: The window to open the file in
    /// - `cx`: The application context
    /// - `path`: The path to the file to open
    pub fn handle_open_file_from_cli(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        path: PathBuf,
    ) {
        log::debug!("Handling file open from CLI: {}", path.display());
        self.do_open_file(window, cx, path);
    }

    /// Handle dropping external file system paths into this window.
    ///
    /// ### Behavior
    /// - Opens dropped files in new tabs (or focuses existing tabs via `do_open_file`)
    /// - Ignores non-file entries (e.g. directories)
    /// - Deduplicates duplicate paths within the same drop gesture
    ///
    /// ### Arguments
    /// - `paths`: Paths provided by GPUI external file drop
    /// - `window`: The target window
    /// - `cx`: The application context
    pub fn handle_external_paths_drop(
        &mut self,
        paths: &ExternalPaths,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut dropped_files = Vec::new();
        let mut seen = HashSet::new();
        let mut skipped_non_files = 0usize;
        for path in paths.paths() {
            if !path.is_file() {
                skipped_non_files += 1;
                continue;
            }
            if seen.insert(path.clone()) {
                dropped_files.push(path.clone());
            }
        }
        if dropped_files.is_empty() {
            if skipped_non_files > 0 {
                window.push_notification(
                    (
                        NotificationType::Info,
                        SharedString::from("Dropped items contain no files to open"),
                    ),
                    cx,
                );
            }
            return;
        }
        log::info!(
            "Opening {} dropped file(s) in window {:?}",
            dropped_files.len(),
            self.window_id
        );
        for file_path in dropped_files {
            self.do_open_file(window, cx, file_path);
        }
        if skipped_non_files > 0 {
            window.push_notification(
                (
                    NotificationType::Info,
                    SharedString::from(format!(
                        "Ignored {skipped_non_files} dropped item(s) that are not files"
                    )),
                ),
                cx,
            );
        }
    }

    /// Process pending files from macOS "Open With" events
    ///
    /// ### Arguments
    /// - `window`: The window to open files in
    /// - `cx`: The application context
    pub fn process_pending_files_from_macos(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let shared = Fulgur::shared_state(cx);
        let should_process_files = cx
            .global::<window_manager::WindowManager>()
            .get_last_focused()
            .is_none_or(|id| id == self.window_id); // If no last focused window, allow this one to process
        let files_to_open = if should_process_files {
            if let Some(mut pending) = shared.pending_files_from_macos.try_lock() {
                if pending.is_empty() {
                    Vec::new()
                } else {
                    log::info!(
                        "Processing {} pending file(s) from macOS open event in window {:?}",
                        pending.len(),
                        self.window_id
                    );
                    pending.drain(..).collect()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        for file_path in files_to_open {
            self.handle_open_file_from_cli(window, cx, file_path);
        }
    }
}

#[cfg(test)]
mod tests {
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use crate::fulgur::{shared_state::SharedAppState, window_manager::WindowManager};

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use crate::fulgur::files::file_operations::test_helpers::invoke_process_pending_files_from_macos;
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use crate::fulgur::files::file_operations::test_helpers::{
        open_window_with_fulgur, setup_test_globals,
    };
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use gpui::BorrowAppContext;
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use gpui::TestAppContext;
    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    use tempfile::TempDir;

    #[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
    #[gpui::test]
    fn test_process_pending_files_from_macos_only_focused_window_drains_queue(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.register(window_id_one, fulgur_one.downgrade());
                manager.register(window_id_two, fulgur_two.downgrade());
            });
        });
        let dir = TempDir::new().expect("failed to create temp dir");
        let file_path = dir.path().join("macos-open-url-focus-test.txt");
        std::fs::write(&file_path, "from open-url event").expect("failed to write temp file");
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            shared
                .pending_files_from_macos
                .lock()
                .push(file_path.clone());
        });
        // Window 1 is not last focused, so it must not drain the queue.
        invoke_process_pending_files_from_macos(cx, window_id_one, &fulgur_one);
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            assert_eq!(
                shared.pending_files_from_macos.lock().len(),
                1,
                "non-focused windows must not consume pending macOS open-url files"
            );
        });
        invoke_process_pending_files_from_macos(cx, window_id_two, &fulgur_two);
        cx.run_until_parked();
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            assert!(
                shared.pending_files_from_macos.lock().is_empty(),
                "focused window should consume pending macOS open-url files"
            );
            // The focused window starts with an empty scratch tab, which the queued file
            // reuses, so the file should be open without adding a new tab.
            let canonical_expected =
                std::fs::canonicalize(&file_path).unwrap_or_else(|_| file_path.clone());
            let has_file = fulgur_two.read(cx).tabs.iter().any(|tab| {
                tab.read(cx)
                    .as_editor()
                    .and_then(|e| e.file_path().cloned())
                    .and_then(|p| std::fs::canonicalize(&p).ok())
                    .is_some_and(|p| p == canonical_expected)
            });
            assert!(has_file, "processing a queued file should open it in a tab");
        });
    }
}
