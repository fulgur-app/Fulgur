use crate::fulgur::settings::{RecentFiles, Settings};
use std::path::PathBuf;

#[test]
fn recent_files_new_creates_empty_list_with_correct_max() {
    let recent_files = RecentFiles::new(5);
    assert_eq!(recent_files.get_files().len(), 0);
    assert_eq!(recent_files.max_files, 5);
}

#[test]
fn recent_files_new_with_zero_max() {
    let recent_files = RecentFiles::new(0);
    assert_eq!(recent_files.get_files().len(), 0);
    assert_eq!(recent_files.max_files, 0);
}

#[test]
fn recent_files_new_with_large_max() {
    let recent_files = RecentFiles::new(1000);
    assert_eq!(recent_files.get_files().len(), 0);
    assert_eq!(recent_files.max_files, 1000);
}

#[test]
fn recent_files_add_file_below_max() {
    let mut recent_files = RecentFiles::new(5);
    let file1 = PathBuf::from("/path/to/file1.txt");
    let file2 = PathBuf::from("/path/to/file2.txt");

    recent_files.add_file(file1.clone());
    assert_eq!(recent_files.get_files().len(), 1);
    assert_eq!(recent_files.get_files()[0], file1);

    recent_files.add_file(file2.clone());
    assert_eq!(recent_files.get_files().len(), 2);
    assert_eq!(recent_files.get_files()[1], file2);
}

#[test]
fn recent_files_add_file_at_max_evicts_oldest() {
    let mut recent_files = RecentFiles::new(3);
    let file1 = PathBuf::from("/path/to/file1.txt");
    let file2 = PathBuf::from("/path/to/file2.txt");
    let file3 = PathBuf::from("/path/to/file3.txt");
    let file4 = PathBuf::from("/path/to/file4.txt");

    recent_files.add_file(file1.clone());
    recent_files.add_file(file2.clone());
    recent_files.add_file(file3.clone());
    assert_eq!(recent_files.get_files().len(), 3);

    // Adding 4th file should evict file1
    recent_files.add_file(file4.clone());
    assert_eq!(recent_files.get_files().len(), 3);
    assert!(!recent_files.get_files().contains(&file1));
    assert_eq!(recent_files.get_files()[0], file2);
    assert_eq!(recent_files.get_files()[1], file3);
    assert_eq!(recent_files.get_files()[2], file4);
}

#[test]
fn recent_files_lru_eviction_behavior() {
    let mut recent_files = RecentFiles::new(3);
    let file1 = PathBuf::from("/path/to/file1.txt");
    let file2 = PathBuf::from("/path/to/file2.txt");
    let file3 = PathBuf::from("/path/to/file3.txt");
    let file4 = PathBuf::from("/path/to/file4.txt");
    let file5 = PathBuf::from("/path/to/file5.txt");

    recent_files.add_file(file1.clone());
    recent_files.add_file(file2.clone());
    recent_files.add_file(file3.clone());
    recent_files.add_file(file4.clone());
    recent_files.add_file(file5.clone());

    // Should keep most recently added files (file3, file4, file5)
    assert_eq!(recent_files.get_files().len(), 3);
    assert_eq!(recent_files.get_files()[0], file3);
    assert_eq!(recent_files.get_files()[1], file4);
    assert_eq!(recent_files.get_files()[2], file5);
    assert!(!recent_files.get_files().contains(&file1));
    assert!(!recent_files.get_files().contains(&file2));
}

#[test]
fn recent_files_remove_existing_file() {
    let mut recent_files = RecentFiles::new(5);
    let file1 = PathBuf::from("/path/to/file1.txt");
    let file2 = PathBuf::from("/path/to/file2.txt");
    let file3 = PathBuf::from("/path/to/file3.txt");

    recent_files.add_file(file1.clone());
    recent_files.add_file(file2.clone());
    recent_files.add_file(file3.clone());
    assert_eq!(recent_files.get_files().len(), 3);

    recent_files.remove_file(file2.clone());
    assert_eq!(recent_files.get_files().len(), 2);
    assert!(!recent_files.get_files().contains(&file2));
    assert_eq!(recent_files.get_files()[0], file1);
    assert_eq!(recent_files.get_files()[1], file3);
}

#[test]
fn recent_files_remove_non_existing_file() {
    let mut recent_files = RecentFiles::new(5);
    let file1 = PathBuf::from("/path/to/file1.txt");
    let file2 = PathBuf::from("/path/to/file2.txt");
    let non_existing = PathBuf::from("/path/to/nonexisting.txt");

    recent_files.add_file(file1.clone());
    recent_files.add_file(file2.clone());
    assert_eq!(recent_files.get_files().len(), 2);

    // Should not change anything
    recent_files.remove_file(non_existing);
    assert_eq!(recent_files.get_files().len(), 2);
    assert_eq!(recent_files.get_files()[0], file1);
    assert_eq!(recent_files.get_files()[1], file2);
}

#[test]
fn recent_files_remove_from_empty_list() {
    let mut recent_files = RecentFiles::new(5);
    let file1 = PathBuf::from("/path/to/file1.txt");

    // Should not panic
    recent_files.remove_file(file1);
    assert_eq!(recent_files.get_files().len(), 0);
}

#[test]
fn recent_files_clear_removes_all_files() {
    let mut recent_files = RecentFiles::new(5);
    let file1 = PathBuf::from("/path/to/file1.txt");
    let file2 = PathBuf::from("/path/to/file2.txt");
    let file3 = PathBuf::from("/path/to/file3.txt");

    recent_files.add_file(file1);
    recent_files.add_file(file2);
    recent_files.add_file(file3);
    assert_eq!(recent_files.get_files().len(), 3);

    recent_files.clear();
    assert_eq!(recent_files.get_files().len(), 0);
}

#[test]
fn recent_files_clear_empty_list() {
    let mut recent_files = RecentFiles::new(5);
    assert_eq!(recent_files.get_files().len(), 0);

    // Should not panic
    recent_files.clear();
    assert_eq!(recent_files.get_files().len(), 0);
}

#[test]
fn settings_load_without_is_deduplication_field() {
    let json = r#"{
        "editor_settings": {
            "show_line_numbers": true,
            "show_indent_guides": true,
            "soft_wrap": false,
            "font_size": 14.0,
            "tab_size": 4,
            "markdown_settings": {
                "show_markdown_preview": true,
                "show_markdown_toolbar": false
            },
            "watch_files": true
        },
        "app_settings": {
            "confirm_exit": true,
            "theme": "Catppuccin Frappe",
            "synchronization_settings": {
                "is_synchronization_activated": true,
                "server_url": "http://localhost:3000",
                "email": "test@example.com",
                "public_key": "age1abc123"
            }
        },
        "recent_files": {
            "files": [],
            "max_files": 10
        }
    }"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    // is_deduplication should default to true when missing from JSON
    assert!(
        settings
            .app_settings
            .synchronization_settings
            .is_deduplication
    );
    // Other settings should be preserved
    assert_eq!(settings.app_settings.theme, "Catppuccin Frappe");
    assert!(
        settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
    );
    assert_eq!(
        settings.app_settings.synchronization_settings.server_url,
        Some("http://localhost:3000".to_string())
    );
    assert_eq!(
        settings.app_settings.synchronization_settings.email,
        Some("test@example.com".to_string())
    );
}

#[cfg(feature = "gpui-test-support")]
mod gpui_settings_versioning_tests {
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{AppContext, BorrowAppContext, Entity, TestAppContext, WindowId};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc, sync::atomic::Ordering};

    /// Initialize shared globals required by `Fulgur::new` for GPUI tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test app context to initialize.
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

    /// Open a test window with a mounted `Fulgur` root view.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test app context used to open the window.
    ///
    /// ### Returns
    /// - `(WindowId, Entity<Fulgur>)`: The opened window ID and owned `Fulgur` entity.
    fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
        let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
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

    /// Register a test window in the shared `WindowManager` global.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test app context.
    /// - `window_id`: The window ID to register.
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

    #[gpui::test]
    fn test_update_and_propagate_settings_increments_shared_version_and_marks_window(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id, fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, window_id, &fulgur);
        cx.update(|cx| {
            let starting_shared_version = cx
                .global::<SharedAppState>()
                .settings_version
                .load(Ordering::Relaxed);
            assert_eq!(starting_shared_version, 0);
            fulgur.update(cx, |this, cx| {
                this.settings.app_settings.theme = "Tokyo Night".into();
                this.settings.app_settings.confirm_exit = false;
                this.settings_changed = false;
                this.update_and_propagate_settings(cx)
                    .expect("settings update should succeed");
                assert_eq!(this.local_settings_version, 1);
                assert!(this.settings_changed);
            });
            let shared = cx.global::<SharedAppState>();
            let shared_version = shared.settings_version.load(Ordering::Relaxed);
            let shared_settings = shared.settings.lock().clone();
            assert_eq!(shared_version, 1);
            assert_eq!(shared_settings.app_settings.theme, "Tokyo Night");
            assert!(!shared_settings.app_settings.confirm_exit);
        });
    }

    #[gpui::test]
    fn test_synchronize_settings_from_other_windows_applies_newer_shared_version(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (_window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (_window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            fulgur_one.update(cx, |this, cx| {
                this.settings.app_settings.theme = "Catppuccin Frappe".into();
                this.settings.editor_settings.show_line_numbers = false;
                this.update_and_propagate_settings(cx)
                    .expect("origin window should publish updated settings");
            });
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let before_sync_theme = fulgur_two.read(cx).settings.app_settings.theme.clone();
            assert_ne!(
                before_sync_theme, "Catppuccin Frappe",
                "target window should not reflect shared settings before synchronization step"
            );
            fulgur_two.update(cx, |this, cx| {
                this.synchronize_settings_from_other_windows(cx);
                assert_eq!(this.settings.app_settings.theme, "Catppuccin Frappe");
                assert!(!this.settings.editor_settings.show_line_numbers);
                assert_eq!(this.local_settings_version, 1);
                assert!(this.settings_changed);
            });
        });
    }

    #[gpui::test]
    fn test_synchronize_settings_from_other_windows_is_noop_when_versions_match(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (_, fulgur) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            {
                let shared = cx.global::<SharedAppState>();
                shared.settings_version.store(5, Ordering::Relaxed);
                let mut shared_settings = shared.settings.lock();
                shared_settings.app_settings.theme = "Shared Theme".into();
            }
            fulgur.update(cx, |this, cx| {
                this.local_settings_version = 5;
                this.settings.app_settings.theme = "Local Theme".into();
                this.settings_changed = false;
                this.synchronize_settings_from_other_windows(cx);
                assert_eq!(this.settings.app_settings.theme, "Local Theme");
                assert_eq!(this.local_settings_version, 5);
                assert!(!this.settings_changed);
            });
        });
    }
}
