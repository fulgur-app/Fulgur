//! Integration tests for State Persistence (save/load roundtrip)
//!
//! These tests verify that `WindowsState` and its nested structures can be
//! written to the `SQLite` session store and read back with full fidelity.
//! They run in CI/CD environments using temporary directories for isolation.
//!
//! ## Platform Independence
//! All file paths are constructed using `PathBuf::push()` to ensure correct
//! path separators on all platforms (/ on Unix, \ on Windows). Never use
//! hardcoded strings like "/path/to/file" or "C:\path\to\file".

// Test fixtures cast small loop indices to f32; precision loss is irrelevant.
#![allow(clippy::cast_precision_loss)]

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Import from the main crate
use fulgur::fulgur::state::{
    SerializedRemoteSpec, SerializedWindowBounds, StateDb, TabContent, TabState, WindowState,
    WindowsState, import_legacy_json,
};

/// Create a temporary file path for testing
///
/// ### Arguments
/// - `temp_dir`: The temporary directory
///
/// ### Returns
/// - `PathBuf`: Path to a state database in the temp directory
fn temp_state_path(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().join("state.db")
}

/// Whether a byte slice contains the given subsequence
///
/// ### Arguments
/// - `haystack`: The bytes to search
/// - `needle`: The subsequence to look for
///
/// ### Returns
/// - `bool`: `true` when `needle` appears anywhere in `haystack`
fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
    haystack
        .windows(needle.len())
        .any(|window| window == needle)
}

/// Create a `TabState` for a file-backed tab (unmodified)
///
/// ### Arguments
/// - `tab_id`: Identity of the tab within its window
///
/// ### Returns
/// - `TabState`: A tab state with a file path and no modified content
fn create_file_tab_unmodified(tab_id: u64) -> TabState {
    let mut path = PathBuf::new();
    path.push("path");
    path.push("to");
    path.push("test.rs");

    TabState {
        tab_id,
        title: "test.rs".to_string(),
        file_path: Some(path),
        content: None,
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
    }
}

/// Create a `TabState` for a file-backed tab (modified)
///
/// ### Arguments
/// - `tab_id`: Identity of the tab within its window
///
/// ### Returns
/// - `TabState`: A tab state with a file path and modified content
fn create_file_tab_modified(tab_id: u64) -> TabState {
    let mut path = PathBuf::new();
    path.push("home");
    path.push("user");
    path.push("document.md");

    TabState {
        tab_id,
        title: "document.md".to_string(),
        file_path: Some(path),
        content: Some(TabContent::from(
            "# Modified Content\n\nThis has unsaved changes.",
        )),
        last_saved: Some("2024-01-15T10:30:00Z".to_string()),
        remote: None,
        log_view: false,
        color_tag: None,
    }
}

/// Create a `TabState` for an unsaved tab
///
/// ### Arguments
/// - `tab_id`: Identity of the tab within its window
///
/// ### Returns
/// - `TabState`: A tab state with no file path (unsaved)
fn create_unsaved_tab(tab_id: u64) -> TabState {
    TabState {
        tab_id,
        title: "Untitled".to_string(),
        file_path: None,
        content: Some(TabContent::from("New file content")),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
    }
}

/// Assert two `TabState` instances are equal
///
/// ### Arguments
/// - `original`: The original tab state
/// - `loaded`: The loaded tab state
/// - `context`: Description of what's being tested
fn assert_tab_state_equal(original: &TabState, loaded: &TabState, context: &str) {
    assert_eq!(original.title, loaded.title, "{context}: title mismatch");
    assert_eq!(
        original.file_path, loaded.file_path,
        "{context}: file_path mismatch"
    );
    assert_eq!(
        original.content, loaded.content,
        "{context}: content mismatch"
    );
    assert_eq!(
        original.last_saved, loaded.last_saved,
        "{context}: last_saved mismatch"
    );
}

/// Assert two `WindowState` instances are equal
///
/// ### Arguments
/// - `original`: The original window state
/// - `loaded`: The loaded window state
/// - `context`: Description of what's being tested
fn assert_window_state_equal(original: &WindowState, loaded: &WindowState, context: &str) {
    assert_eq!(
        original.tabs.len(),
        loaded.tabs.len(),
        "{context}: tab count mismatch"
    );
    for (i, (orig_tab, loaded_tab)) in original.tabs.iter().zip(loaded.tabs.iter()).enumerate() {
        assert_tab_state_equal(orig_tab, loaded_tab, &format!("{context} - tab {i}"));
    }
    assert_eq!(
        original.active_tab_index, loaded.active_tab_index,
        "{context}: active_tab_index mismatch"
    );
    assert_eq!(
        original.window_bounds.state, loaded.window_bounds.state,
        "{context}: window_bounds.state mismatch"
    );
    assert!(
        (original.window_bounds.x - loaded.window_bounds.x).abs() < f32::EPSILON,
        "{context}: window_bounds.x mismatch"
    );
    assert!(
        (original.window_bounds.y - loaded.window_bounds.y).abs() < f32::EPSILON,
        "{context}: window_bounds.y mismatch"
    );
    assert!(
        (original.window_bounds.width - loaded.window_bounds.width).abs() < f32::EPSILON,
        "{context}: window_bounds.width mismatch"
    );
    assert!(
        (original.window_bounds.height - loaded.window_bounds.height).abs() < f32::EPSILON,
        "{context}: window_bounds.height mismatch"
    );
    assert_eq!(
        original.window_bounds.display_id, loaded.window_bounds.display_id,
        "{context}: window_bounds.display_id mismatch"
    );
}

#[test]
fn test_state_roundtrip_single_window_with_mixed_tabs() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![WindowState {
            window_id: 1,
            tabs: vec![
                create_file_tab_unmodified(0),
                create_file_tab_modified(1),
                create_unsaved_tab(2),
            ],
            active_tab_index: Some(1),
            window_bounds: SerializedWindowBounds {
                state: "Windowed".to_string(),
                x: 150.0,
                y: 200.0,
                width: 1024.0,
                height: 768.0,
                display_id: Some(1),
            },
        }],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(
        original.windows.len(),
        loaded.windows.len(),
        "Window count should match"
    );
    assert_window_state_equal(
        &original.windows[0],
        &loaded.windows[0],
        "Single window with mixed tabs",
    );
}

#[test]
fn test_state_roundtrip_multiple_windows() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![
            WindowState {
                window_id: 2,
                tabs: vec![create_file_tab_unmodified(0), create_file_tab_modified(1)],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds {
                    state: "Windowed".to_string(),
                    x: 100.0,
                    y: 100.0,
                    width: 1200.0,
                    height: 800.0,
                    display_id: Some(1),
                },
            },
            WindowState {
                window_id: 3,
                tabs: vec![create_unsaved_tab(0)],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds {
                    state: "Maximized".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                    display_id: Some(2),
                },
            },
            WindowState {
                window_id: 4,
                tabs: vec![
                    create_file_tab_unmodified(0),
                    create_unsaved_tab(1),
                    create_file_tab_modified(2),
                ],
                active_tab_index: Some(2),
                window_bounds: SerializedWindowBounds {
                    state: "Fullscreen".to_string(),
                    x: 0.0,
                    y: 0.0,
                    width: 2560.0,
                    height: 1440.0,
                    display_id: None,
                },
            },
        ],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(
        original.windows.len(),
        loaded.windows.len(),
        "Should have 3 windows"
    );
    for (i, (orig_window, loaded_window)) in original
        .windows
        .iter()
        .zip(loaded.windows.iter())
        .enumerate()
    {
        assert_window_state_equal(orig_window, loaded_window, &format!("Window {i}"));
    }
}

#[test]
fn test_state_roundtrip_empty_windows() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState { windows: vec![] };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(
        original.windows.len(),
        loaded.windows.len(),
        "Empty windows vec should roundtrip"
    );
}

#[test]
fn test_state_roundtrip_window_no_tabs() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![WindowState {
            window_id: 5,
            tabs: vec![],
            active_tab_index: None,
            window_bounds: SerializedWindowBounds::default(),
        }],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(loaded.windows.len(), 1);
    assert_eq!(loaded.windows[0].tabs.len(), 0);
    assert_eq!(loaded.windows[0].active_tab_index, None);
}

#[test]
fn test_window_bounds_variants() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let windowed = SerializedWindowBounds {
        state: "Windowed".to_string(),
        x: 100.0,
        y: 200.0,
        width: 800.0,
        height: 600.0,
        display_id: Some(1),
    };
    let maximized = SerializedWindowBounds {
        state: "Maximized".to_string(),
        x: 0.0,
        y: 0.0,
        width: 1920.0,
        height: 1080.0,
        display_id: Some(1),
    };
    let fullscreen = SerializedWindowBounds {
        state: "Fullscreen".to_string(),
        x: 0.0,
        y: 0.0,
        width: 2560.0,
        height: 1440.0,
        display_id: Some(2),
    };
    for (label, bounds) in [
        ("Windowed", windowed),
        ("Maximized", maximized),
        ("Fullscreen", fullscreen),
    ] {
        let original = WindowsState {
            windows: vec![WindowState {
                window_id: 6,
                tabs: vec![],
                active_tab_index: None,
                window_bounds: bounds.clone(),
            }],
        };
        original
            .save_to_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to save {label} state"));
        let loaded = WindowsState::load_from_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to load {label}"));
        assert_eq!(loaded.windows[0].window_bounds.state, bounds.state);
        assert!((loaded.windows[0].window_bounds.x - bounds.x).abs() < f32::EPSILON);
        assert!((loaded.windows[0].window_bounds.y - bounds.y).abs() < f32::EPSILON);
        assert!((loaded.windows[0].window_bounds.width - bounds.width).abs() < f32::EPSILON);
        assert!((loaded.windows[0].window_bounds.height - bounds.height).abs() < f32::EPSILON);
        assert_eq!(
            loaded.windows[0].window_bounds.display_id,
            bounds.display_id
        );
    }
}

#[test]
fn test_window_bounds_default_values() {
    let default_bounds = SerializedWindowBounds::default();
    assert_eq!(default_bounds.state, "Windowed");
    assert!((default_bounds.x - 100.0_f32).abs() < f32::EPSILON);
    assert!((default_bounds.y - 100.0_f32).abs() < f32::EPSILON);
    assert!((default_bounds.width - 1200.0_f32).abs() < f32::EPSILON);
    assert!((default_bounds.height - 800.0_f32).abs() < f32::EPSILON);
    assert_eq!(default_bounds.display_id, None);
}

#[test]
fn test_state_roundtrip_with_real_temp_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let file1_path = temp_dir.path().join("real_file1.txt");
    let file2_path = temp_dir.path().join("real_file2.rs");
    fs::write(&file1_path, "File 1 content").expect("Failed to create temp file 1");
    fs::write(&file2_path, "File 2 content").expect("Failed to create temp file 2");
    let original = WindowsState {
        windows: vec![WindowState {
            window_id: 7,
            tabs: vec![
                TabState {
                    tab_id: 0,
                    title: "real_file1.txt".to_string(),
                    file_path: Some(file1_path.clone()),
                    content: None,
                    last_saved: None,
                    remote: None,
                    log_view: false,
                    color_tag: None,
                },
                TabState {
                    tab_id: 1,
                    title: "real_file2.rs".to_string(),
                    file_path: Some(file2_path.clone()),
                    content: Some(TabContent::from("Modified!")),
                    last_saved: Some("2024-01-01T00:00:00Z".to_string()),
                    remote: None,
                    log_view: false,
                    color_tag: None,
                },
            ],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(
        loaded.windows[0].tabs[0].file_path,
        Some(file1_path.clone())
    );
    assert_eq!(
        loaded.windows[0].tabs[1].file_path,
        Some(file2_path.clone())
    );
    assert!(file1_path.exists(), "File 1 should still exist");
    assert!(file2_path.exists(), "File 2 should still exist");
}

#[test]
fn test_state_roundtrip_with_unicode_content() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let unicode_content = "Hello 世界 🦀 Здравствуй مرحبا";
    let unicode_title = "文档.txt";
    let mut unicode_path = PathBuf::new();
    unicode_path.push("path");
    unicode_path.push("to");
    unicode_path.push("文档.txt");
    let original = WindowsState {
        windows: vec![WindowState {
            window_id: 8,
            tabs: vec![TabState {
                tab_id: 2,
                title: unicode_title.to_string(),
                file_path: Some(unicode_path),
                content: Some(TabContent::from(unicode_content)),
                last_saved: Some("2024-01-01T00:00:00Z".to_string()),
                remote: None,
                log_view: false,
                color_tag: None,
            }],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state with unicode");
    let loaded =
        WindowsState::load_from_path(&state_path).expect("Failed to load state with unicode");
    assert_eq!(loaded.windows[0].tabs[0].title, unicode_title);
    assert_eq!(
        loaded.windows[0].tabs[0].content.as_ref().unwrap(),
        unicode_content
    );
}

#[test]
fn test_legacy_json_document_is_imported_with_default_window_bounds() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    #[cfg(target_os = "windows")]
    let file_path_json = r"path\\to\\test.txt";
    #[cfg(not(target_os = "windows"))]
    let file_path_json = "path/to/test.txt";
    // A minimal document as written by Fulgur 0.10 and earlier: no identity, and
    // no window bounds either.
    let minimal_json = format!(
        r#"{{
        "windows": [
            {{
                "tabs": [
                    {{
                        "title": "test.txt",
                        "file_path": "{file_path_json}",
                        "content": null,
                        "last_saved": null
                    }}
                ],
                "active_tab_index": 0
            }}
        ]
    }}"#
    );
    let legacy_path = temp_dir.path().join("state.json");
    fs::write(&legacy_path, &minimal_json).expect("Failed to write minimal JSON");

    let mut db = StateDb::open(&state_path).expect("Failed to open state database");
    let imported = import_legacy_json(&legacy_path, &mut db).expect("import legacy");
    assert_eq!(imported, 1);
    let loaded = db.load().expect("Failed to load imported state");

    assert_eq!(loaded.windows.len(), 1);
    assert_eq!(loaded.windows[0].tabs.len(), 1);
    assert!(
        loaded.windows[0].window_id > 0,
        "an imported window must be given an identity"
    );
    // Window bounds should have default values
    assert_eq!(loaded.windows[0].window_bounds.state, "Windowed");
    assert!((loaded.windows[0].window_bounds.width - 1200.0_f32).abs() < f32::EPSILON);
}

#[test]
fn test_state_multiple_save_load_cycles() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let mut state = WindowsState {
        windows: vec![WindowState {
            window_id: 9,
            tabs: vec![create_file_tab_unmodified(0)],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    };
    for i in 0..5 {
        state
            .save_to_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to save on iteration {i}"));
        let loaded = WindowsState::load_from_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to load on iteration {i}"));
        assert_eq!(state.windows.len(), loaded.windows.len());
        assert_window_state_equal(&state.windows[0], &loaded.windows[0], &format!("Cycle {i}"));
        // Each cycle appends one more tab, and every tab in a window needs its
        // own identity for the store to address its row.
        let next_tab_id = u64::try_from(state.windows[0].tabs.len()).expect("tab count fits u64");
        state.windows[0].tabs.push(create_unsaved_tab(next_tab_id));
        state.windows[0].window_bounds.x += 10.0;
        state = loaded;
    }
}

#[test]
fn test_state_load_nonexistent_file_returns_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nonexistent_path = temp_dir.path().join("nested").join("does_not_exist.db");
    let result = WindowsState::load_from_path(&nonexistent_path);
    assert!(
        result.is_err(),
        "Loading non-existent file should return an error"
    );
}

#[test]
fn test_state_load_of_a_file_that_is_not_a_database_returns_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let invalid_path = temp_state_path(&temp_dir);
    fs::write(&invalid_path, "{ this is not a database }").expect("Failed to write invalid file");
    let result = WindowsState::load_from_path(&invalid_path);
    assert!(
        result.is_err(),
        "Loading a file that is not a database should return an error"
    );
}

#[test]
fn test_state_is_written_as_a_sqlite_database() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let state = WindowsState {
        windows: vec![WindowState {
            window_id: 10,
            tabs: vec![create_file_tab_modified(0)],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds {
                state: "Windowed".to_string(),
                x: 100.0,
                y: 200.0,
                width: 800.0,
                height: 600.0,
                display_id: Some(1),
            },
        }],
    };
    state.save_to_path(&state_path).expect("Failed to save");

    let bytes = fs::read(&state_path).expect("Failed to read state file");
    assert!(
        bytes.starts_with(b"SQLite format 3\0"),
        "the session store must be a SQLite database"
    );
    // Byte 18 of the header is the file format write version: 1 for a rollback
    // journal, 2 for WAL. WAL is what gives atomic multi-statement saves and
    // crash recovery, so it has to survive in the file itself, not just on the
    // connection that created it.
    assert_eq!(
        bytes[18], 2,
        "the database must be in WAL mode, which persists in the file header"
    );

    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(loaded.windows.len(), 1);
    assert_eq!(loaded.windows[0].window_id, 10);
    assert_eq!(loaded.windows[0].tabs[0].title, "document.md");
    assert_eq!(loaded.windows[0].window_bounds.state, "Windowed");
    assert!((loaded.windows[0].window_bounds.x - 100.0_f32).abs() < f32::EPSILON);
    assert_eq!(loaded.windows[0].window_bounds.display_id, Some(1));
}

#[test]
fn test_state_roundtrip_preserves_remote_spec_without_password_fields() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![WindowState {
            window_id: 11,
            tabs: vec![TabState {
                tab_id: 3,
                title: "remote.txt".to_string(),
                file_path: None,
                content: Some(TabContent::from("cached remote content")),
                last_saved: None,
                remote: Some(SerializedRemoteSpec {
                    host: "example.com".to_string(),
                    port: 2222,
                    user: "alice".to_string(),
                    path: "/srv/remote.txt".to_string(),
                }),
                log_view: false,
                color_tag: None,
            }],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    };

    original
        .save_to_path(&state_path)
        .expect("Failed to save remote state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load remote state");

    assert_eq!(loaded.windows.len(), 1);
    let loaded_remote = loaded.windows[0].tabs[0]
        .remote
        .as_ref()
        .expect("remote metadata should be restored");
    assert_eq!(loaded_remote.host, "example.com");
    assert_eq!(loaded_remote.port, 2222);
    assert_eq!(loaded_remote.user, "alice");
    assert_eq!(loaded_remote.path, "/srv/remote.txt");

    // Scan the raw database bytes, not just the decoded rows, so a password
    // leaking into any column or into a stale page would still be caught.
    let stored = fs::read(&state_path).expect("Failed to read state file");
    assert!(
        !contains_bytes(&stored, b"password"),
        "the session store must not persist SSH passwords"
    );
}

#[test]
fn test_state_preserves_window_order() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let mut windows = Vec::new();
    for i in 0u32..5 {
        windows.push(WindowState {
            window_id: i64::from(i) + 12,
            tabs: vec![TabState {
                tab_id: 4,
                title: format!("Window {i} Marker"),
                file_path: None,
                content: Some(TabContent::from(format!("This is window number {i}"))),
                last_saved: None,
                remote: None,
                log_view: false,
                color_tag: None,
            }],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds {
                state: "Windowed".to_string(),
                x: (i as f32) * 100.0,
                y: (i as f32) * 100.0,
                width: 800.0,
                height: 600.0,
                display_id: Some(i),
            },
        });
    }
    let original = WindowsState { windows };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(loaded.windows.len(), 5);
    for i in 0..5 {
        assert_eq!(
            loaded.windows[i].tabs[0].title,
            format!("Window {i} Marker"),
            "Window order should be preserved"
        );
        assert!(
            (loaded.windows[i].window_bounds.x - (i as f32) * 100.0).abs() < f32::EPSILON,
            "Window position should match index"
        );
    }
}

#[test]
fn test_state_windows_with_different_active_tabs() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![
            WindowState {
                window_id: 13,
                tabs: vec![
                    create_file_tab_unmodified(0),
                    create_file_tab_modified(1),
                    create_unsaved_tab(2),
                ],
                active_tab_index: Some(0), // First tab active
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                window_id: 14,
                tabs: vec![
                    create_file_tab_unmodified(0),
                    create_file_tab_modified(1),
                    create_unsaved_tab(2),
                ],
                active_tab_index: Some(1), // Second tab active
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                window_id: 15,
                tabs: vec![
                    create_file_tab_unmodified(0),
                    create_file_tab_modified(1),
                    create_unsaved_tab(2),
                ],
                active_tab_index: Some(2), // Third tab active
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                window_id: 16,
                tabs: vec![
                    create_file_tab_unmodified(0),
                    create_file_tab_modified(1),
                    create_unsaved_tab(2),
                ],
                active_tab_index: None, // No active tab
                window_bounds: SerializedWindowBounds::default(),
            },
        ],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(loaded.windows.len(), 4);
    assert_eq!(loaded.windows[0].active_tab_index, Some(0));
    assert_eq!(loaded.windows[1].active_tab_index, Some(1));
    assert_eq!(loaded.windows[2].active_tab_index, Some(2));
    assert_eq!(loaded.windows[3].active_tab_index, None);
}

#[test]
fn test_state_windows_on_different_displays() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![
            WindowState {
                window_id: 17,
                tabs: vec![create_unsaved_tab(0)],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds {
                    state: "Windowed".to_string(),
                    x: 100.0,
                    y: 100.0,
                    width: 1200.0,
                    height: 800.0,
                    display_id: Some(1), // Primary display
                },
            },
            WindowState {
                window_id: 18,
                tabs: vec![create_unsaved_tab(0)],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds {
                    state: "Maximized".to_string(),
                    x: 1920.0,
                    y: 0.0,
                    width: 1920.0,
                    height: 1080.0,
                    display_id: Some(2), // Secondary display
                },
            },
            WindowState {
                window_id: 19,
                tabs: vec![create_unsaved_tab(0)],
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds {
                    state: "Fullscreen".to_string(),
                    x: 3840.0,
                    y: 0.0,
                    width: 2560.0,
                    height: 1440.0,
                    display_id: Some(3), // Tertiary display
                },
            },
        ],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(loaded.windows.len(), 3);
    assert_eq!(loaded.windows[0].window_bounds.display_id, Some(1));
    assert_eq!(loaded.windows[1].window_bounds.display_id, Some(2));
    assert_eq!(loaded.windows[2].window_bounds.display_id, Some(3));
    assert!((loaded.windows[0].window_bounds.x - 100.0_f32).abs() < f32::EPSILON);
    assert!((loaded.windows[1].window_bounds.x - 1920.0_f32).abs() < f32::EPSILON);
    assert!((loaded.windows[2].window_bounds.x - 3840.0_f32).abs() < f32::EPSILON);
}

#[test]
fn test_state_mixed_window_and_tab_counts() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![
            WindowState {
                window_id: 20,
                tabs: vec![create_file_tab_unmodified(0)], // 1 tab
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                window_id: 21,
                tabs: vec![
                    create_file_tab_unmodified(0),
                    create_file_tab_modified(1),
                    create_unsaved_tab(2),
                    create_file_tab_unmodified(3),
                    create_file_tab_modified(4),
                ], // 5 tabs
                active_tab_index: Some(2),
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                window_id: 22,
                tabs: vec![], // 0 tabs
                active_tab_index: None,
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                window_id: 23,
                tabs: vec![create_file_tab_unmodified(0), create_unsaved_tab(1)], // 2 tabs
                active_tab_index: Some(1),
                window_bounds: SerializedWindowBounds::default(),
            },
        ],
    };
    original
        .save_to_path(&state_path)
        .expect("Failed to save state");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");
    assert_eq!(loaded.windows.len(), 4);
    assert_eq!(loaded.windows[0].tabs.len(), 1);
    assert_eq!(loaded.windows[1].tabs.len(), 5);
    assert_eq!(loaded.windows[2].tabs.len(), 0);
    assert_eq!(loaded.windows[3].tabs.len(), 2);
    assert_eq!(loaded.windows[0].active_tab_index, Some(0));
    assert_eq!(loaded.windows[1].active_tab_index, Some(2));
    assert_eq!(loaded.windows[2].active_tab_index, None);
    assert_eq!(loaded.windows[3].active_tab_index, Some(1));
}
