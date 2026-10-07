use crate::fulgur::{Fulgur, shared_state::SharedAppState, window_manager};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{App, Context, ExternalPaths, SharedString, Window};
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

    /// Open, in this window, every file queued from outside the app
    ///
    /// ### Arguments
    /// - `window`: The window to open files in
    /// - `cx`: The application context
    pub fn process_pending_external_files(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let files_to_open: Vec<PathBuf> = Fulgur::shared_state(cx)
            .pending_files_from_macos
            .lock()
            .drain(..)
            .collect();
        if files_to_open.is_empty() {
            return;
        }
        log::info!(
            "Opening {} file(s) requested from outside the app in window {:?}",
            files_to_open.len(),
            self.window_id
        );
        for file_path in files_to_open {
            self.handle_open_file_from_cli(window, cx, file_path);
        }
    }

    /// Deliver the queued external open requests to a window
    ///
    /// External requests are macOS open events, command-line file arguments
    /// and messages forwarded by another Fulgur process. They are queued in
    /// `SharedAppState` and delivered here, outside of any render pass, to the
    /// last focused window (or any open window), which is brought to the front.
    /// When no window exists yet the requests stay queued: window creation calls
    /// this again once the window and its `Root` are mounted.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn deliver_external_open_requests(cx: &mut App) {
        if !Self::has_pending_external_open_requests(cx) {
            return;
        }
        let window_manager = cx.global::<window_manager::WindowManager>();
        let target = window_manager
            .get_last_focused()
            .into_iter()
            .chain(window_manager.get_all_window_ids())
            .find_map(|window_id| {
                let fulgur = window_manager.get_window(window_id)?.upgrade()?;
                Some((window_id, fulgur))
            });
        let Some((window_id, fulgur)) = target else {
            log::debug!("No window yet for external open requests, keeping them queued");
            return;
        };
        let Some(handle) = cx
            .windows()
            .into_iter()
            .find(|handle| handle.window_id() == window_id)
        else {
            return;
        };
        if let Err(e) = handle.update(cx, |_, window, cx| {
            window.activate_window();
            fulgur.update(cx, |this, cx| {
                this.process_pending_external_files(window, cx);
                #[cfg(any(target_os = "windows", target_os = "linux"))]
                this.process_pending_ipc_commands(window, cx);
            });
        }) {
            log::error!("Failed to deliver external open requests to window {window_id:?}: {e}");
        }
    }

    /// Whether files or commands from outside the app are waiting to be delivered
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: At least one file or IPC command is queued
    /// - `false`: Both queues are empty
    fn has_pending_external_open_requests(cx: &App) -> bool {
        let shared = cx.global::<SharedAppState>();
        let has_files = !shared.pending_files_from_macos.lock().is_empty();
        #[cfg(any(target_os = "windows", target_os = "linux"))]
        let has_commands = !shared.pending_ipc_commands.lock().is_empty();
        #[cfg(not(any(target_os = "windows", target_os = "linux")))]
        let has_commands = false;
        has_files || has_commands
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur, WindowInit,
        files::file_operations::test_helpers::{open_window_with_fulgur, setup_test_globals},
        shared_state::SharedAppState,
        ui::tabs::editor_tab::TabLocation,
        window_manager::WindowManager,
    };
    use gpui_kit::component::{Root, WindowExt};
    use gpui_kit::{
        AppContext, BorrowAppContext, Entity, Focusable, TestAppContext, VisualTestContext,
        WindowOptions,
    };
    use std::{cell::RefCell, path::Path};
    use tempfile::TempDir;

    /// Queue a file as if it came from outside the app (open event, argument, other instance).
    ///
    /// ### Arguments
    /// - `cx`: The test application context
    /// - `path`: The file to queue
    fn queue_external_file(cx: &mut TestAppContext, path: &Path) {
        cx.update(|cx| {
            cx.global::<SharedAppState>()
                .pending_files_from_macos
                .lock()
                .push(path.to_path_buf());
        });
    }

    /// Whether a window has a tab for the given file.
    ///
    /// ### Arguments
    /// - `fulgur`: The window's Fulgur entity
    /// - `path`: The file to look for
    /// - `cx`: The test application context
    ///
    /// ### Returns
    /// - `true`: One of the window's tabs points at `path`
    /// - `false`: No tab points at `path`
    fn has_tab_for(fulgur: &Entity<Fulgur>, path: &Path, cx: &mut TestAppContext) -> bool {
        let expected = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
        cx.update(|cx| {
            fulgur.read(cx).tabs.iter().any(|tab| {
                tab.read(cx)
                    .as_editor()
                    .and_then(|editor| editor.file_path().cloned())
                    .and_then(|p| std::fs::canonicalize(p).ok())
                    .is_some_and(|p| p == expected)
            })
        })
    }

    #[gpui_kit::test]
    fn test_deliver_external_open_requests_opens_files_in_last_focused_window(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.register(window_id_one, fulgur_one.downgrade());
                manager.register(window_id_two, fulgur_two.downgrade());
                manager.set_focused(window_id_two);
            });
        });
        let dir = TempDir::new().expect("failed to create temp dir");
        let file_path = dir.path().join("external-open-test.txt");
        std::fs::write(&file_path, "from outside").expect("failed to write temp file");
        queue_external_file(cx, &file_path);

        cx.update(Fulgur::deliver_external_open_requests);
        cx.run_until_parked();

        cx.update(|cx| {
            assert!(
                cx.global::<SharedAppState>()
                    .pending_files_from_macos
                    .lock()
                    .is_empty(),
                "delivering must drain the queue"
            );
        });
        assert!(
            has_tab_for(&fulgur_two, &file_path, cx),
            "the last focused window must open the queued file"
        );
        assert!(
            !has_tab_for(&fulgur_one, &file_path, cx),
            "other windows must not open the queued file"
        );
    }

    #[gpui_kit::test]
    fn test_deliver_external_open_requests_keeps_files_queued_without_window(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let file_path = dir.path().join("queued-before-window.txt");
        std::fs::write(&file_path, "early").expect("failed to write temp file");
        queue_external_file(cx, &file_path);

        cx.update(Fulgur::deliver_external_open_requests);

        cx.update(|cx| {
            assert_eq!(
                cx.global::<SharedAppState>()
                    .pending_files_from_macos
                    .lock()
                    .len(),
                1,
                "a request arriving before any window must wait for the first window"
            );
        });
    }

    /// Reproduces a cold start: the session restores a modified buffer for a file
    /// while another tab is active, and the same file is opened from outside.
    #[gpui_kit::test]
    fn test_cold_start_reopen_of_modified_file_prompt_keeps_keyboard_focus(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let dir = TempDir::new().expect("failed to create temp dir");
        let path = dir.path().join("PRD.md");
        std::fs::write(&path, "content on disk").expect("failed to write disk version");
        let path = path
            .canonicalize()
            .expect("failed to canonicalize temp file");
        queue_external_file(cx, &path);

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let handle = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, WindowInit::Empty);
                    fulgur.update(cx, |this, cx| {
                        this.new_tab(window, cx);
                        this.tabs[1].clone().update(cx, |tab, cx| {
                            let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                            editor_tab.location = TabLocation::Local(path.clone());
                            editor_tab.content.update(cx, |state, cx| {
                                state.set_value("local unsaved edits", window, cx);
                            });
                            editor_tab.set_original_content_from_str("content on disk");
                            editor_tab.modified = true;
                        });
                        // As restored: another tab is active and its activation is deferred.
                        let restored_active = this.tabs[0].read(cx).id();
                        this.active_tab_id = Some(restored_active);
                        this.pending_initial_active_tab = Some(restored_active);
                        this.focus_active_tab(window, cx);
                    });
                    cx.update_global::<WindowManager, _>(|manager, _| {
                        manager.register(window_id, fulgur.downgrade());
                    });
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        cx.update(Fulgur::deliver_external_open_requests);

        let mut visual_cx = VisualTestContext::from_window(handle.into(), cx);
        for _ in 0..3 {
            visual_cx.run_until_parked();
            visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        }
        let (active_tab_id, reopened_tab_id, editor_focused, has_dialog) =
            visual_cx.update(|window, cx| {
                let this = fulgur.read(cx);
                let reopened = this.tabs[1].read(cx);
                let editor_focused = reopened
                    .as_editor()
                    .expect("expected an editor tab")
                    .content
                    .read(cx)
                    .focus_handle(cx)
                    .is_focused(window);
                (
                    this.active_tab_id,
                    reopened.id(),
                    editor_focused,
                    window.has_active_dialog(cx),
                )
            });
        assert_eq!(
            active_tab_id,
            Some(reopened_tab_id),
            "the reopened file must become the active tab"
        );
        assert!(has_dialog, "reopening a modified file must prompt");
        assert!(
            !editor_focused,
            "the deferred startup activation must not pull focus out of the prompt"
        );

        visual_cx.simulate_keystrokes("enter");
        visual_cx.run_until_parked();
        let (has_dialog, text) = visual_cx.update(|window, cx| {
            let text = fulgur.read(cx).tabs[1]
                .read(cx)
                .as_editor()
                .expect("expected an editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            (window.has_active_dialog(cx), text)
        });
        assert!(!has_dialog, "Enter must confirm the prompt");
        assert_eq!(
            text, "content on disk",
            "confirming the prompt must reload the file from disk"
        );
    }
}
