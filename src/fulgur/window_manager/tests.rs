use super::WindowManager;
use crate::fulgur::{
    Fulgur, editor_tab::TabLocation, settings::Settings, shared_state::SharedAppState,
};
use gpui::{
    AppContext, BorrowAppContext, Entity, SharedString, TestAppContext, WindowId, WindowOptions,
};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::{
    cell::RefCell,
    path::{Path, PathBuf},
    sync::Arc,
};

/// Initialize test globals required by `Fulgur::new`.
///
/// ### Arguments
/// - `cx`: The GPUI test application context to initialize.
fn setup_test_globals(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files));
        cx.set_global(WindowManager::new());
    });
}

/// Open a window that owns a `Fulgur` entity and return both identifiers.
///
/// ### Arguments
/// - `cx`: The GPUI test application context used to open the window.
///
/// ### Returns
/// - `(WindowId, Entity<Fulgur>)`: The window ID and the associated `Fulgur` entity.
fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
    let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            let window_id = window.window_handle().window_id();
            let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
            *window_id_slot.borrow_mut() = Some(window_id);
            *fulgur_slot.borrow_mut() = Some(fulgur.clone());
            cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
        })
        .expect("failed to open test window");
    });
    (
        window_id_slot
            .into_inner()
            .expect("failed to capture test window id"),
        fulgur_slot
            .into_inner()
            .expect("failed to capture test Fulgur entity"),
    )
}

/// Build an OS-agnostic temporary path for file lookup tests.
///
/// ### Arguments
/// - `file_name`: The file name to append to the platform temp directory.
///
/// ### Returns
/// - `PathBuf`: A path rooted under `std::env::temp_dir()`.
fn temp_test_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

/// Register a window entity inside the global `WindowManager`.
///
/// ### Arguments
/// - `cx`: The GPUI test application context.
/// - `window_id`: The ID of the window to register.
/// - `fulgur`: The `Fulgur` entity associated with the window.
fn register_window_in_global_manager(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
) {
    cx.update(|cx| {
        cx.update_global::<WindowManager, _>(|manager, _| {
            manager.register(window_id, fulgur.downgrade());
        });
    });
}

/// Invoke `on_window_close_requested` against a specific window in tests.
///
/// ### Arguments
/// - `cx`: The GPUI test application context.
/// - `window_id`: The target window ID to run close handling against.
/// - `fulgur`: The `Fulgur` entity that owns the close handler.
///
/// ### Returns
/// - `bool`: The return value from `Fulgur::on_window_close_requested`.
fn invoke_window_close_requested(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
) -> bool {
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == window_id {
                return handle
                    .update(cx, |_, window, cx| {
                        fulgur.update(cx, |this, cx| this.on_window_close_requested(window, cx))
                    })
                    .expect("failed to run close handler on test window");
            }
        }
        panic!("failed to locate target test window by id");
    })
}

/// Invoke `do_open_file` against a specific window in tests.
///
/// ### Arguments
/// - `cx`: The GPUI test application context.
/// - `window_id`: The target window ID where the open request should run.
/// - `fulgur`: The `Fulgur` entity that owns the open handler.
/// - `path`: The file path to open.
fn invoke_do_open_file(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
    path: &Path,
) {
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == window_id {
                handle
                    .update(cx, |_, window, cx| {
                        fulgur.update(cx, |this, cx| {
                            this.do_open_file(window, cx, path.to_path_buf())
                        });
                    })
                    .expect("failed to run do_open_file on test window");
                return;
            }
        }
        panic!("failed to locate target test window by id");
    });
}

/// Invoke `process_window_state_updates` against a specific window in tests.
///
/// ### Arguments
/// - `cx`: The GPUI test application context.
/// - `window_id`: The target window ID where the render-phase processing should run.
/// - `fulgur`: The `Fulgur` entity that owns the processing method.
fn invoke_process_window_state_updates(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
) {
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == window_id {
                handle
                    .update(cx, |_, window, cx| {
                        fulgur.update(cx, |this, cx| {
                            this.process_window_state_updates(window, cx);
                        });
                    })
                    .expect("failed to run process_window_state_updates on test window");
                return;
            }
        }
        panic!("failed to locate target test window by id");
    });
}

/// Invoke `handle_dock_activate_tab` against a specific window in tests.
///
/// ### Arguments
/// - `cx`: The GPUI test application context.
/// - `window_id`: The target window ID where the dock action should run.
/// - `fulgur`: The `Fulgur` entity that owns the dock handler.
/// - `path`: The file path carried by `DockActivateTab`.
fn invoke_dock_activate_tab(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
    path: &Path,
) {
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == window_id {
                handle
                    .update(cx, |_, window, cx| {
                        fulgur.update(cx, |this, cx| {
                            let action =
                                crate::fulgur::ui::menus::DockActivateTab(path.to_path_buf());
                            this.handle_dock_activate_tab(&action, window, cx);
                        });
                    })
                    .expect("failed to run DockActivateTab on test window");
                return;
            }
        }
        panic!("failed to locate target test window by id");
    });
}

/// Invoke `handle_dock_activate_tab_by_title` against a specific window in tests.
///
/// ### Arguments
/// - `cx`: The GPUI test application context.
/// - `window_id`: The target window ID where the dock action should run.
/// - `fulgur`: The `Fulgur` entity that owns the dock handler.
/// - `title`: The tab title carried by `DockActivateTabByTitle`.
fn invoke_dock_activate_tab_by_title(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
    title: &SharedString,
) {
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == window_id {
                handle
                    .update(cx, |_, window, cx| {
                        fulgur.update(cx, |this, cx| {
                            let action =
                                crate::fulgur::ui::menus::DockActivateTabByTitle(title.clone());
                            this.handle_dock_activate_tab_by_title(&action, window, cx);
                        });
                    })
                    .expect("failed to run DockActivateTabByTitle on test window");
                return;
            }
        }
        panic!("failed to locate target test window by id");
    });
}

#[gpui::test]
fn test_register_unregister_and_focus_tracking(cx: &mut TestAppContext) {
    setup_test_globals(cx);
    let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
    let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
    cx.update(|_| {
        let mut manager = WindowManager::new();
        assert_eq!(manager.window_count(), 0);
        assert_eq!(manager.get_last_focused(), None);
        manager.register(window_id_one, fulgur_one.downgrade());
        assert_eq!(manager.window_count(), 1);
        assert_eq!(manager.get_last_focused(), Some(window_id_one));
        assert!(manager.get_window(window_id_one).is_some());
        manager.register(window_id_two, fulgur_two.downgrade());
        assert_eq!(manager.window_count(), 2);
        assert_eq!(manager.get_last_focused(), Some(window_id_two));
        let window_ids = manager.get_all_window_ids();
        assert_eq!(window_ids.len(), 2);
        assert!(window_ids.contains(&window_id_one));
        assert!(window_ids.contains(&window_id_two));
        assert_eq!(manager.get_all_windows().len(), 2);
        manager.set_focused(window_id_one);
        assert_eq!(manager.get_last_focused(), Some(window_id_one));
        manager.unregister(window_id_one);
        assert_eq!(manager.window_count(), 1);
        assert_eq!(manager.get_last_focused(), Some(window_id_two));
        assert!(manager.get_window(window_id_one).is_none());
        // Focusing an unregistered window must leave focus unchanged.
        manager.set_focused(window_id_one);
        assert_eq!(manager.get_last_focused(), Some(window_id_two));
        manager.unregister(window_id_two);
        assert_eq!(manager.window_count(), 0);
        assert_eq!(manager.get_last_focused(), None);
    });
}

#[gpui::test]
fn test_find_window_with_file_returns_other_window_with_matching_tab(cx: &mut TestAppContext) {
    setup_test_globals(cx);
    let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
    let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
    let target_path = temp_test_path("fulgur_window_manager_cross_window_lookup.md");
    cx.update(|cx| {
        other_fulgur.update(cx, |fulgur, _| {
            let editor = fulgur
                .tabs
                .first_mut()
                .and_then(|tab| tab.as_editor_mut())
                .expect("expected initial editor tab");
            editor.location = TabLocation::Local(target_path.clone());
        });
        let mut manager = WindowManager::new();
        manager.register(current_window_id, current_fulgur.downgrade());
        manager.register(other_window_id, other_fulgur.downgrade());
        let found_window_id = manager.find_window_with_file(&target_path, current_window_id, cx);
        assert_eq!(found_window_id, Some(other_window_id));
    });
}

#[gpui::test]
fn test_find_window_with_file_skips_current_window_and_returns_none_on_miss(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
    let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
    let current_only_path = temp_test_path("fulgur_window_manager_current_only.rs");
    let missing_path = temp_test_path("fulgur_window_manager_missing.rs");
    cx.update(|cx| {
        current_fulgur.update(cx, |fulgur, _| {
            let editor = fulgur
                .tabs
                .first_mut()
                .and_then(|tab| tab.as_editor_mut())
                .expect("expected initial editor tab");
            editor.location = TabLocation::Local(current_only_path.clone());
        });
        let mut manager = WindowManager::new();
        manager.register(current_window_id, current_fulgur.downgrade());
        manager.register(other_window_id, other_fulgur.downgrade());
        let found_in_other =
            manager.find_window_with_file(&current_only_path, current_window_id, cx);
        assert_eq!(
            found_in_other, None,
            "current window must be ignored during cross-window lookup"
        );
        let missing = manager.find_window_with_file(&missing_path, current_window_id, cx);
        assert_eq!(
            missing, None,
            "missing files should return no matching window"
        );
    });
}

#[gpui::test]
fn test_on_window_close_requested_last_window_with_confirm_exit_blocks_close(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (window_id, fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, window_id, &fulgur);
    cx.update(|cx| {
        fulgur.update(cx, |this, _| {
            this.settings.app_settings.confirm_exit = true;
        });
    });
    let should_close = invoke_window_close_requested(cx, window_id, &fulgur);
    assert!(
        !should_close,
        "last window should remain open when confirm_exit is enabled"
    );
    cx.update(|cx| {
        let manager = cx.global::<WindowManager>();
        assert_eq!(manager.window_count(), 1);
        assert!(manager.get_window(window_id).is_some());
    });
}

#[gpui::test]
fn test_on_window_close_requested_last_window_without_confirm_exit_closes_and_unregisters(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (window_id, fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, window_id, &fulgur);
    cx.update(|cx| {
        fulgur.update(cx, |this, _| {
            this.settings.app_settings.confirm_exit = false;
        });
    });
    let should_close = invoke_window_close_requested(cx, window_id, &fulgur);
    assert!(
        should_close,
        "last window should close when confirm_exit is disabled"
    );
    cx.update(|cx| {
        let manager = cx.global::<WindowManager>();
        assert_eq!(manager.window_count(), 0);
        assert!(manager.get_window(window_id).is_none());
    });
}

#[gpui::test]
fn test_on_window_close_requested_non_last_window_closes_even_with_confirm_exit_enabled(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
    let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, window_id_one, &fulgur_one);
    register_window_in_global_manager(cx, window_id_two, &fulgur_two);
    cx.update(|cx| {
        fulgur_two.update(cx, |this, _| {
            this.settings.app_settings.confirm_exit = true;
        });
    });
    let should_close = invoke_window_close_requested(cx, window_id_two, &fulgur_two);
    assert!(
        should_close,
        "non-last windows should close without quit confirmation flow"
    );
    cx.update(|cx| {
        let manager = cx.global::<WindowManager>();
        assert_eq!(manager.window_count(), 1);
        assert!(manager.get_window(window_id_one).is_some());
        assert!(manager.get_window(window_id_two).is_none());
        assert_eq!(manager.get_last_focused(), Some(window_id_one));
    });
}

#[gpui::test]
fn test_process_window_state_updates_drains_local_and_sync_pending_notifications(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (window_id, fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, window_id, &fulgur);

    cx.update(|cx| {
        fulgur.update(cx, |this, cx| {
            this.pending_notification = Some((
                NotificationType::Warning,
                "pending from current window".into(),
            ));
            *this.shared_state(cx).sync_state.pending_notification.lock() = Some((
                NotificationType::Success,
                "pending from sync background task".into(),
            ));
        });
    });

    invoke_process_window_state_updates(cx, window_id, &fulgur);

    cx.update(|cx| {
        fulgur.update(cx, |this, cx| {
            assert!(
                this.pending_notification.is_none(),
                "window-local pending notification must be drained during render processing"
            );
            assert!(
                this.shared_state(cx)
                    .sync_state
                    .pending_notification
                    .lock()
                    .is_none(),
                "sync pending notification must be drained during render processing"
            );
        });

        let manager = cx.global::<WindowManager>();
        assert_eq!(
            manager.get_last_focused(),
            Some(window_id),
            "process_window_state_updates should keep focus tracking in sync"
        );
    });
}

#[gpui::test]
fn test_do_open_file_does_not_open_duplicate_when_file_exists_in_another_window(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
    let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, current_window_id, &current_fulgur);
    register_window_in_global_manager(cx, other_window_id, &other_fulgur);
    let shared_path = temp_test_path("fulgur_cross_window_existing_file.rs");
    cx.update(|cx| {
        other_fulgur.update(cx, |fulgur, _| {
            let editor = fulgur
                .tabs
                .first_mut()
                .and_then(|tab| tab.as_editor_mut())
                .expect("expected initial editor tab");
            editor.location = TabLocation::Local(shared_path.clone());
        });
    });
    let tab_count_before = cx.update(|cx| current_fulgur.read(cx).tabs.len());
    invoke_do_open_file(cx, current_window_id, &current_fulgur, &shared_path);
    cx.run_until_parked();
    let tab_count_after = cx.update(|cx| current_fulgur.read(cx).tabs.len());
    assert_eq!(
        tab_count_after, tab_count_before,
        "opening a file already open in another window should not create a duplicate tab"
    );
}

#[gpui::test]
fn test_dock_activate_tab_transfers_active_tab_to_other_window(cx: &mut TestAppContext) {
    setup_test_globals(cx);
    let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
    let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, current_window_id, &current_fulgur);
    register_window_in_global_manager(cx, other_window_id, &other_fulgur);
    let target_path = temp_test_path("fulgur_dock_focus_transfer.rs");
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == other_window_id {
                handle
                    .update(cx, |_, window, cx| {
                        other_fulgur.update(cx, |this, cx| {
                            this.new_tab(window, cx);
                            this.active_tab_index = Some(0);
                            if let Some(editor) =
                                this.tabs.get_mut(1).and_then(|tab| tab.as_editor_mut())
                            {
                                editor.location = TabLocation::Local(target_path.clone());
                                editor.title = "dock-target.rs".into();
                            }
                        });
                    })
                    .expect("failed to prepare target window tab state");
                break;
            }
        }
    });

    invoke_dock_activate_tab(cx, current_window_id, &current_fulgur, &target_path);
    cx.run_until_parked();

    cx.update(|cx| {
        let other = other_fulgur.read(cx);
        assert_eq!(
            other.active_tab_index,
            Some(1),
            "dock activation by path should activate the matching tab in the other window"
        );
    });
}

#[gpui::test]
fn test_dock_activate_tab_by_title_transfers_active_tab_to_other_window(cx: &mut TestAppContext) {
    setup_test_globals(cx);
    let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
    let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, current_window_id, &current_fulgur);
    register_window_in_global_manager(cx, other_window_id, &other_fulgur);
    let target_title: SharedString = "cross-window-title-target".into();
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == other_window_id {
                handle
                    .update(cx, |_, window, cx| {
                        other_fulgur.update(cx, |this, cx| {
                            this.new_tab(window, cx);
                            this.active_tab_index = Some(0);
                            if let Some(editor) =
                                this.tabs.get_mut(1).and_then(|tab| tab.as_editor_mut())
                            {
                                editor.title = target_title.clone();
                            }
                        });
                    })
                    .expect("failed to prepare target window title state");
                break;
            }
        }
    });
    invoke_dock_activate_tab_by_title(cx, current_window_id, &current_fulgur, &target_title);
    cx.run_until_parked();

    cx.update(|cx| {
        let other = other_fulgur.read(cx);
        assert_eq!(
            other.active_tab_index,
            Some(1),
            "dock activation by title should activate the matching tab in the other window"
        );
    });
}

#[gpui::test]
fn test_update_window_menu_fingerprint_bumps_revision_only_on_change(cx: &mut TestAppContext) {
    setup_test_globals(cx);
    let (window_id, fulgur) = open_window_with_fulgur(cx);
    cx.update(|_| {
        let mut manager = WindowManager::new();
        manager.register(window_id, fulgur.downgrade());
        let revision_after_register = manager.menu_state_revision();
        assert!(
            manager.update_window_menu_fingerprint(window_id, 42),
            "first fingerprint publication must bump menu revision"
        );
        let revision_after_first_publish = manager.menu_state_revision();
        assert_ne!(
            revision_after_first_publish, revision_after_register,
            "menu revision should change after first fingerprint publication"
        );
        assert!(
            !manager.update_window_menu_fingerprint(window_id, 42),
            "publishing an unchanged fingerprint must not bump menu revision"
        );
        assert_eq!(
            manager.menu_state_revision(),
            revision_after_first_publish,
            "menu revision should remain stable when fingerprint does not change"
        );
        assert!(
            manager.update_window_menu_fingerprint(window_id, 99),
            "changed fingerprint must bump menu revision"
        );
        assert_eq!(
            manager.get_window_menu_fingerprint(window_id),
            Some(99),
            "window manager should store the latest published fingerprint"
        );
    });
}

#[gpui::test]
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn test_process_window_state_updates_updates_menu_fingerprint_on_tab_change(
    cx: &mut TestAppContext,
) {
    setup_test_globals(cx);
    let (window_id, fulgur) = open_window_with_fulgur(cx);
    register_window_in_global_manager(cx, window_id, &fulgur);

    invoke_process_window_state_updates(cx, window_id, &fulgur);
    let (initial_revision, initial_fingerprint) = cx.update(|cx| {
        let manager = cx.global::<WindowManager>();
        (
            manager.menu_state_revision(),
            manager.get_window_menu_fingerprint(window_id),
        )
    });
    assert!(
        initial_fingerprint.is_some(),
        "first render processing should publish a menu fingerprint"
    );

    invoke_process_window_state_updates(cx, window_id, &fulgur);
    let revision_after_no_change =
        cx.update(|cx| cx.global::<WindowManager>().menu_state_revision());
    assert_eq!(
        revision_after_no_change, initial_revision,
        "menu revision should not change when tab state is unchanged"
    );

    cx.update(|cx| {
        fulgur.update(cx, |this, _| {
            let editor = this
                .tabs
                .first_mut()
                .and_then(|tab| tab.as_editor_mut())
                .expect("expected initial editor tab");
            editor.title = "menu-state-changed.md".into();
        });
    });
    invoke_process_window_state_updates(cx, window_id, &fulgur);
    let (updated_revision, updated_fingerprint) = cx.update(|cx| {
        let manager = cx.global::<WindowManager>();
        (
            manager.menu_state_revision(),
            manager.get_window_menu_fingerprint(window_id),
        )
    });
    assert_ne!(
        updated_revision, initial_revision,
        "menu revision should change after tab state update"
    );
    assert_ne!(
        updated_fingerprint, initial_fingerprint,
        "published fingerprint should change after tab state update"
    );
}
