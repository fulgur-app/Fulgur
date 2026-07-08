use crate::fulgur::settings::{RecentFiles, ServerProfile, Settings};
use std::path::PathBuf;
use tempfile::TempDir;

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

    recent_files.remove_file(&file2);
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
    recent_files.remove_file(&non_existing);
    assert_eq!(recent_files.get_files().len(), 2);
    assert_eq!(recent_files.get_files()[0], file1);
    assert_eq!(recent_files.get_files()[1], file2);
}

#[test]
fn recent_files_remove_from_empty_list() {
    let mut recent_files = RecentFiles::new(5);
    let file1 = PathBuf::from("/path/to/file1.txt");

    // Should not panic
    recent_files.remove_file(&file1);
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
fn settings_load_without_is_deduplication_field_migrates_legacy_to_profile() {
    // For legacy single-server JSON (Fulgur <= 0.7.0).
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
    assert_eq!(settings.app_settings.theme, "Catppuccin Frappe");
    assert!(
        settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
    );
    let profiles = &settings.app_settings.synchronization_settings.profiles;
    assert_eq!(
        profiles.len(),
        1,
        "legacy JSON should migrate to a single profile"
    );
    let profile = &profiles[0];
    assert_eq!(profile.name, "Fulgurant");
    assert!(profile.is_active);
    assert!(
        profile.is_deduplication,
        "is_deduplication should default to true when missing from legacy JSON"
    );
    assert_eq!(
        profile.server_url,
        Some("http://localhost:3000".to_string())
    );
    assert_eq!(profile.email, Some("test@example.com".to_string()));
    assert_eq!(profile.public_key, Some("age1abc123".to_string()));
}

#[test]
fn settings_load_with_new_profiles_shape() {
    // New shape with explicit profiles array round-trips without migration.
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
            "theme": "Default Light",
            "synchronization_settings": {
                "is_synchronization_activated": true,
                "profiles": [
                    {
                        "id": "abc-123",
                        "name": "Home",
                        "is_active": true,
                        "server_url": "https://home.example",
                        "email": "me@home.example",
                        "public_key": "age1home",
                        "is_deduplication": false
                    },
                    {
                        "id": "def-456",
                        "name": "Work",
                        "is_active": false,
                        "server_url": null,
                        "email": null,
                        "is_deduplication": true
                    }
                ]
            }
        },
        "recent_files": {
            "files": [],
            "max_files": 10
        }
    }"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    let profiles = &settings.app_settings.synchronization_settings.profiles;
    assert_eq!(profiles.len(), 2);
    assert_eq!(profiles[0].id, "abc-123");
    assert_eq!(profiles[0].name, "Home");
    assert!(profiles[0].is_active);
    assert!(!profiles[0].is_deduplication);
    assert_eq!(profiles[1].id, "def-456");
    assert_eq!(profiles[1].name, "Work");
    assert!(!profiles[1].is_active);
    assert!(profiles[1].is_deduplication);
}

#[test]
fn settings_load_with_no_legacy_data_yields_empty_profiles() {
    // Legacy JSON with `is_synchronization_activated: false` and no other
    // legacy fields produces an empty profiles list (no profile to migrate).
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
            "theme": "Default Light",
            "synchronization_settings": {
                "is_synchronization_activated": false
            }
        },
        "recent_files": {
            "files": [],
            "max_files": 10
        }
    }"#;
    let settings: Settings = serde_json::from_str(json).unwrap();
    assert!(
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .is_empty(),
        "no legacy data should produce empty profiles"
    );
    assert!(
        !settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
    );
}

#[test]
fn validate_clamps_font_size_below_minimum() {
    let mut settings = Settings::new();
    settings.editor_settings.font_size = 2.0;
    settings.validate();
    assert!((settings.editor_settings.font_size - 6.0_f32).abs() < f32::EPSILON);
}

#[test]
fn validate_clamps_font_size_above_maximum() {
    let mut settings = Settings::new();
    settings.editor_settings.font_size = 200.0;
    settings.validate();
    assert!((settings.editor_settings.font_size - 72.0_f32).abs() < f32::EPSILON);
}

#[test]
fn validate_leaves_font_size_unchanged_when_valid() {
    let mut settings = Settings::new();
    settings.editor_settings.font_size = 16.0;
    settings.validate();
    assert!((settings.editor_settings.font_size - 16.0_f32).abs() < f32::EPSILON);
}

#[test]
fn validate_replaces_non_finite_font_size_with_default() {
    let mut settings = Settings::new();
    settings.editor_settings.font_size = f32::NAN;
    settings.validate();
    assert!((settings.editor_settings.font_size - 14.0_f32).abs() < f32::EPSILON);
}

#[test]
fn validate_clamps_tab_size_zero_to_minimum() {
    let mut settings = Settings::new();
    settings.editor_settings.tab_size = 0;
    settings.validate();
    assert_eq!(settings.editor_settings.tab_size, 1);
}

#[test]
fn validate_clamps_tab_size_above_maximum() {
    let mut settings = Settings::new();
    settings.editor_settings.tab_size = 100;
    settings.validate();
    assert_eq!(settings.editor_settings.tab_size, 16);
}

#[test]
fn validate_leaves_tab_size_unchanged_when_valid() {
    let mut settings = Settings::new();
    settings.editor_settings.tab_size = 4;
    settings.validate();
    assert_eq!(settings.editor_settings.tab_size, 4);
}

#[test]
fn validate_clamps_max_recent_files_above_maximum() {
    let mut settings = Settings::new();
    settings.recent_files.max_files = 500;
    settings.validate();
    assert_eq!(settings.recent_files.max_files, 100);
}

#[test]
fn validate_leaves_max_recent_files_unchanged_when_within_range() {
    let mut settings = Settings::new();
    settings.recent_files.max_files = 10;
    settings.validate();
    assert_eq!(settings.recent_files.max_files, 10);
}

/// Helper: insert a single profile into the settings and return a mutable reference.
fn push_profile(settings: &mut Settings) -> &mut ServerProfile {
    settings
        .app_settings
        .synchronization_settings
        .profiles
        .push(ServerProfile::new("Fulgurant"));
    &mut settings.app_settings.synchronization_settings.profiles[0]
}

#[test]
fn validate_clears_malformed_server_url() {
    let mut settings = Settings::new();
    push_profile(&mut settings).server_url = Some("not a url".to_string());
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].server_url,
        None
    );
}

#[test]
fn validate_keeps_valid_server_url() {
    let mut settings = Settings::new();
    push_profile(&mut settings).server_url = Some("https://sync.example.com".to_string());
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].server_url,
        Some("https://sync.example.com".to_string())
    );
}

#[test]
fn validate_keeps_none_server_url_unchanged() {
    let mut settings = Settings::new();
    push_profile(&mut settings).server_url = None;
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].server_url,
        None
    );
}

#[test]
fn validate_clears_email_without_at_sign() {
    let mut settings = Settings::new();
    push_profile(&mut settings).email = Some("notanemail".to_string());
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].email,
        None
    );
}

#[test]
fn validate_clears_email_with_at_at_start() {
    let mut settings = Settings::new();
    push_profile(&mut settings).email = Some("@nodomain".to_string());
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].email,
        None
    );
}

#[test]
fn validate_clears_email_missing_tld() {
    let mut settings = Settings::new();
    push_profile(&mut settings).email = Some("user@nodot".to_string());
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].email,
        None
    );
}

#[test]
fn validate_keeps_valid_email() {
    let mut settings = Settings::new();
    push_profile(&mut settings).email = Some("user@example.com".to_string());
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].email,
        Some("user@example.com".to_string())
    );
}

#[test]
fn validate_keeps_none_email_unchanged() {
    let mut settings = Settings::new();
    push_profile(&mut settings).email = None;
    settings.validate();
    assert_eq!(
        settings.app_settings.synchronization_settings.profiles[0].email,
        None
    );
}

#[test]
fn validate_truncates_profiles_above_max() {
    use crate::fulgur::settings::MAX_PROFILES;
    let mut settings = Settings::new();
    for i in 0..(MAX_PROFILES + 5) {
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .push(ServerProfile::new(format!("Profile{i}")));
    }
    settings.validate();
    assert_eq!(
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .len(),
        MAX_PROFILES,
        "validate must truncate profile vector to the maximum allowed"
    );
}

#[test]
fn synchronization_settings_find_profile_returns_match() {
    let mut settings = Settings::new();
    let profile = ServerProfile::new("Server A");
    let id = profile.id.clone();
    settings
        .app_settings
        .synchronization_settings
        .profiles
        .push(profile);
    let found = settings
        .app_settings
        .synchronization_settings
        .find_profile(&id);
    assert!(
        found.is_some(),
        "lookup by id must find the inserted profile"
    );
    let missing = settings
        .app_settings
        .synchronization_settings
        .find_profile("does-not-exist");
    assert!(missing.is_none(), "unknown id must return None");
}

#[test]
fn synchronization_settings_find_profile_mut_allows_mutation() {
    let mut settings = Settings::new();
    let profile = ServerProfile::new("Original");
    let id = profile.id.clone();
    settings
        .app_settings
        .synchronization_settings
        .profiles
        .push(profile);
    let found = settings
        .app_settings
        .synchronization_settings
        .find_profile_mut(&id)
        .expect("profile should be present");
    found.name = "Renamed".into();
    let stored = settings
        .app_settings
        .synchronization_settings
        .find_profile(&id)
        .expect("profile should still be present after mutation");
    assert_eq!(stored.name, "Renamed");
}

#[test]
fn synchronization_settings_name_collides_is_case_insensitive_and_trimmed() {
    let mut settings = Settings::new();
    settings
        .app_settings
        .synchronization_settings
        .profiles
        .push(ServerProfile::new("Fulgurant"));
    assert!(
        settings
            .app_settings
            .synchronization_settings
            .name_collides("  fulgurant  ", None),
        "case-insensitive comparison must match after trimming"
    );
    assert!(
        !settings
            .app_settings
            .synchronization_settings
            .name_collides("home", None),
        "different name must not collide"
    );
}

#[test]
fn synchronization_settings_name_collides_excludes_self() {
    let mut settings = Settings::new();
    let profile = ServerProfile::new("Office");
    let id = profile.id.clone();
    settings
        .app_settings
        .synchronization_settings
        .profiles
        .push(profile);
    assert!(
        !settings
            .app_settings
            .synchronization_settings
            .name_collides("Office", Some(&id)),
        "exclude_id must skip the profile being edited"
    );
    assert!(
        settings
            .app_settings
            .synchronization_settings
            .name_collides("Office", Some("other-id")),
        "non-matching exclude_id must still detect the collision"
    );
}

#[test]
fn load_from_path_persists_legacy_synchronization_migration() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let legacy_json = r#"{
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
            "theme": "Default Light",
            "synchronization_settings": {
                "is_synchronization_activated": true,
                "server_url": "https://example.com",
                "email": "user@example.com",
                "public_key": "age1abc"
            }
        },
        "recent_files": {
            "files": [],
            "max_files": 10
        }
    }"#;
    std::fs::write(&path, legacy_json).unwrap();

    let first = Settings::load_from_path(&path).unwrap();
    let migrated_id = first
        .app_settings
        .synchronization_settings
        .profiles
        .first()
        .expect("legacy load should produce one profile")
        .id
        .clone();

    let on_disk_value: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    let sync = on_disk_value
        .get("app_settings")
        .and_then(|v| v.get("synchronization_settings"))
        .expect("synchronization_settings should still exist");
    assert!(
        sync.get("profiles").is_some(),
        "settings file must be rewritten with the new `profiles` shape"
    );
    assert!(
        sync.get("server_url").is_none(),
        "legacy `server_url` must be gone after migration"
    );

    let second = Settings::load_from_path(&path).unwrap();
    let second_id = second
        .app_settings
        .synchronization_settings
        .profiles
        .first()
        .expect("second load should still find the profile")
        .id
        .clone();
    assert_eq!(
        migrated_id, second_id,
        "profile id must be stable across reloads once the new shape is on disk"
    );
}

#[test]
fn load_from_path_does_not_rewrite_new_shape_settings() {
    // Sanity check: the migration-rewrite path must not touch settings files
    // that are already in the new `profiles` shape.
    use crate::fulgur::settings::ServerProfile;
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let mut settings = Settings::new();
    settings
        .app_settings
        .synchronization_settings
        .profiles
        .push(ServerProfile::new("Already Migrated"));
    settings.save_to_path(&path).unwrap();
    let backup_path = crate::fulgur::utils::atomic_write::backup_path_for(&path);
    assert!(
        !backup_path.exists(),
        "no backup expected before any rewrite occurs"
    );

    let _loaded = Settings::load_from_path(&path).unwrap();
    assert!(
        !backup_path.exists(),
        "loading new-shape settings must not trigger a rewrite (which would create a backup)"
    );
}

#[test]
fn save_to_path_creates_backup_of_previous_file() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let backup = dir.path().join("settings.json.bak");

    let mut settings = Settings::new();
    settings.editor_settings.font_size = 16.0;
    settings.save_to_path(&path).unwrap();
    assert!(!backup.exists(), "no backup before second save");

    settings.editor_settings.font_size = 20.0;
    settings.save_to_path(&path).unwrap();
    assert!(backup.exists(), "backup created on second save");

    let backup_settings = Settings::load_from_path(&backup).unwrap();
    assert!((backup_settings.editor_settings.font_size - 16.0_f32).abs() < f32::EPSILON);
}

#[test]
fn load_from_path_recovers_settings_from_backup_when_primary_is_corrupted() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let backup = dir.path().join("settings.json.bak");

    let mut settings = Settings::new();
    settings.editor_settings.font_size = 18.0;
    Settings::save_to_path(&settings, &backup).unwrap();

    std::fs::write(&path, b"not valid json").unwrap();

    let recovered = Settings::load_from_path(&path).unwrap();
    assert!((recovered.editor_settings.font_size - 18.0_f32).abs() < f32::EPSILON);
}

#[test]
fn load_from_path_returns_error_when_both_primary_and_backup_are_corrupted() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("settings.json");
    let backup = dir.path().join("settings.json.bak");

    std::fs::write(&path, b"bad primary").unwrap();
    std::fs::write(&backup, b"bad backup").unwrap();

    let result = Settings::load_from_path(&path);
    assert!(result.is_err());
}

#[cfg(feature = "gpui-test-support")]
mod gpui_settings_propagation_tests {
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{AppContext, BorrowAppContext, Entity, TestAppContext, WindowId, WindowOptions};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

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
            cx.set_global(SharedAppState::new(settings, pending_files, None));
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
    fn test_update_and_propagate_settings_publishes_to_shared_state_and_marks_window(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id, fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, window_id, &fulgur);
        cx.update(|cx| {
            fulgur.update(cx, |this, cx| {
                this.settings.app_settings.theme = "Tokyo Night".into();
                this.settings.app_settings.confirm_exit = false;
                this.settings_changed = false;
                this.update_and_propagate_settings(cx)
                    .expect("settings update should succeed");
                assert!(this.settings_changed);
            });
            let shared_settings = cx.global::<SharedAppState>().settings.clone();
            assert_eq!(shared_settings.app_settings.theme, "Tokyo Night");
            assert!(!shared_settings.app_settings.confirm_exit);
        });
    }

    #[gpui::test]
    fn test_settings_observer_applies_shared_settings_to_other_windows(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let (_window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (_window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            fulgur_two.update(cx, |this, _cx| {
                this.settings_changed = false;
            });
            fulgur_one.update(cx, |this, cx| {
                this.settings.app_settings.theme = "Catppuccin Frappe".into();
                this.settings.editor_settings.show_line_numbers = false;
                this.update_and_propagate_settings(cx)
                    .expect("origin window should publish updated settings");
            });
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let target = fulgur_two.read(cx);
            assert_eq!(target.settings.app_settings.theme, "Catppuccin Frappe");
            assert!(!target.settings.editor_settings.show_line_numbers);
            assert!(
                !target.settings_changed,
                "the render triggered by the observer should have consumed settings_changed by propagating settings to tabs"
            );
        });
    }

    #[gpui::test]
    fn test_theme_only_global_update_does_not_mark_settings_changed(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let (_, fulgur) = open_window_with_fulgur(cx);
        cx.update(|cx| {
            fulgur.update(cx, |this, _cx| {
                this.settings_changed = false;
            });
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.themes = None;
            });
        });
        cx.run_until_parked();
        cx.update(|cx| {
            let this = fulgur.read(cx);
            assert!(
                !this.settings_changed,
                "a global update that leaves settings identical should not trigger settings propagation"
            );
        });
    }
}
