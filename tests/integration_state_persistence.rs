//! Integration tests for State Persistence (save/load roundtrip)
//!
//! These tests verify that WindowsState and its nested structures can be
//! serialized to JSON, saved to disk, and deserialized back with full fidelity.
//! They run in CI/CD environments using temporary directories for isolation.
//!
//! ## Platform Independence
//! All file paths are constructed using `PathBuf::push()` to ensure correct
//! path separators on all platforms (/ on Unix, \ on Windows). Never use
//! hardcoded strings like "/path/to/file" or "C:\path\to\file".

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

// Import from the main crate
use fulgur::fulgur::state_persistence::{
    SerializedWindowBounds, TabState, WindowState, WindowsState,
};

/// Create a temporary file path for testing
///
/// ### Arguments
/// - `temp_dir`: The temporary directory
///
/// ### Returns
/// - `PathBuf`: Path to a state.json file in the temp directory
fn temp_state_path(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().join("state.json")
}

/// Create a TabState for a file-backed tab (unmodified)
///
/// ### Returns
/// - `TabState`: A tab state with a file path and no modified content
fn create_file_tab_unmodified() -> TabState {
    let mut path = PathBuf::new();
    path.push("path");
    path.push("to");
    path.push("test.rs");

    TabState {
        title: "test.rs".to_string(),
        file_path: Some(path),
        content: None,
        last_saved: None,
    }
}

/// Create a TabState for a file-backed tab (modified)
///
/// ### Returns
/// - `TabState`: A tab state with a file path and modified content
fn create_file_tab_modified() -> TabState {
    let mut path = PathBuf::new();
    path.push("home");
    path.push("user");
    path.push("document.md");

    TabState {
        title: "document.md".to_string(),
        file_path: Some(path),
        content: Some("# Modified Content\n\nThis has unsaved changes.".to_string()),
        last_saved: Some("2024-01-15T10:30:00Z".to_string()),
    }
}

/// Create a TabState for an unsaved tab
///
/// ### Returns
/// - `TabState`: A tab state with no file path (unsaved)
fn create_unsaved_tab() -> TabState {
    TabState {
        title: "Untitled".to_string(),
        file_path: None,
        content: Some("New file content".to_string()),
        last_saved: None,
    }
}

/// Assert two TabState instances are equal
///
/// ### Arguments
/// - `original`: The original tab state
/// - `loaded`: The loaded tab state
/// - `context`: Description of what's being tested
fn assert_tab_state_equal(original: &TabState, loaded: &TabState, context: &str) {
    assert_eq!(original.title, loaded.title, "{}: title mismatch", context);
    assert_eq!(
        original.file_path, loaded.file_path,
        "{}: file_path mismatch",
        context
    );
    assert_eq!(
        original.content, loaded.content,
        "{}: content mismatch",
        context
    );
    assert_eq!(
        original.last_saved, loaded.last_saved,
        "{}: last_saved mismatch",
        context
    );
}

/// Assert two WindowState instances are equal
///
/// ### Arguments
/// - `original`: The original window state
/// - `loaded`: The loaded window state
/// - `context`: Description of what's being tested
fn assert_window_state_equal(original: &WindowState, loaded: &WindowState, context: &str) {
    assert_eq!(
        original.tabs.len(),
        loaded.tabs.len(),
        "{}: tab count mismatch",
        context
    );
    for (i, (orig_tab, loaded_tab)) in original.tabs.iter().zip(loaded.tabs.iter()).enumerate() {
        assert_tab_state_equal(orig_tab, loaded_tab, &format!("{} - tab {}", context, i));
    }
    assert_eq!(
        original.active_tab_index, loaded.active_tab_index,
        "{}: active_tab_index mismatch",
        context
    );
    assert_eq!(
        original.window_bounds.state, loaded.window_bounds.state,
        "{}: window_bounds.state mismatch",
        context
    );
    assert_eq!(
        original.window_bounds.x, loaded.window_bounds.x,
        "{}: window_bounds.x mismatch",
        context
    );
    assert_eq!(
        original.window_bounds.y, loaded.window_bounds.y,
        "{}: window_bounds.y mismatch",
        context
    );
    assert_eq!(
        original.window_bounds.width, loaded.window_bounds.width,
        "{}: window_bounds.width mismatch",
        context
    );
    assert_eq!(
        original.window_bounds.height, loaded.window_bounds.height,
        "{}: window_bounds.height mismatch",
        context
    );
    assert_eq!(
        original.window_bounds.display_id, loaded.window_bounds.display_id,
        "{}: window_bounds.display_id mismatch",
        context
    );
}

#[test]
fn test_state_roundtrip_single_window_with_mixed_tabs() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![WindowState {
            tabs: vec![
                create_file_tab_unmodified(),
                create_file_tab_modified(),
                create_unsaved_tab(),
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
                tabs: vec![create_file_tab_unmodified(), create_file_tab_modified()],
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
                tabs: vec![create_unsaved_tab()],
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
                tabs: vec![
                    create_file_tab_unmodified(),
                    create_unsaved_tab(),
                    create_file_tab_modified(),
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
        assert_window_state_equal(orig_window, loaded_window, &format!("Window {}", i));
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
                tabs: vec![],
                active_tab_index: None,
                window_bounds: bounds.clone(),
            }],
        };
        original
            .save_to_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to save {} state", label));
        let loaded = WindowsState::load_from_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to load {}", label));
        assert_eq!(loaded.windows[0].window_bounds.state, bounds.state);
        assert_eq!(loaded.windows[0].window_bounds.x, bounds.x);
        assert_eq!(loaded.windows[0].window_bounds.y, bounds.y);
        assert_eq!(loaded.windows[0].window_bounds.width, bounds.width);
        assert_eq!(loaded.windows[0].window_bounds.height, bounds.height);
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
    assert_eq!(default_bounds.x, 100.0);
    assert_eq!(default_bounds.y, 100.0);
    assert_eq!(default_bounds.width, 1200.0);
    assert_eq!(default_bounds.height, 800.0);
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
            tabs: vec![
                TabState {
                    title: "real_file1.txt".to_string(),
                    file_path: Some(file1_path.clone()),
                    content: None,
                    last_saved: None,
                },
                TabState {
                    title: "real_file2.rs".to_string(),
                    file_path: Some(file2_path.clone()),
                    content: Some("Modified!".to_string()),
                    last_saved: Some("2024-01-01T00:00:00Z".to_string()),
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
            tabs: vec![TabState {
                title: unicode_title.to_string(),
                file_path: Some(unicode_path),
                content: Some(unicode_content.to_string()),
                last_saved: Some("2024-01-01T00:00:00Z".to_string()),
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
fn test_state_backward_compatibility_missing_window_bounds() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    #[cfg(target_os = "windows")]
    let file_path_json = r#"path\\to\\test.txt"#;
    #[cfg(not(target_os = "windows"))]
    let file_path_json = "path/to/test.txt";
    let minimal_json = format!(
        r#"{{
        "windows": [
            {{
                "tabs": [
                    {{
                        "title": "test.txt",
                        "file_path": "{}",
                        "content": null,
                        "last_saved": null
                    }}
                ],
                "active_tab_index": 0
            }}
        ]
    }}"#,
        file_path_json
    );

    fs::write(&state_path, &minimal_json).expect("Failed to write minimal JSON");
    let loaded = WindowsState::load_from_path(&state_path).expect("Failed to load state");

    assert_eq!(loaded.windows.len(), 1);
    assert_eq!(loaded.windows[0].tabs.len(), 1);
    // Window bounds should have default values
    assert_eq!(loaded.windows[0].window_bounds.state, "Windowed");
    assert_eq!(loaded.windows[0].window_bounds.width, 1200.0);
}

#[test]
fn test_state_multiple_save_load_cycles() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let mut state = WindowsState {
        windows: vec![WindowState {
            tabs: vec![create_file_tab_unmodified()],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    };
    for i in 0..5 {
        state
            .save_to_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to save on iteration {}", i));
        let loaded = WindowsState::load_from_path(&state_path)
            .unwrap_or_else(|_| panic!("Failed to load on iteration {}", i));
        assert_eq!(state.windows.len(), loaded.windows.len());
        assert_window_state_equal(
            &state.windows[0],
            &loaded.windows[0],
            &format!("Cycle {}", i),
        );
        state.windows[0].tabs.push(create_unsaved_tab());
        state.windows[0].window_bounds.x += 10.0;
        state = loaded;
    }
}

#[test]
fn test_state_load_nonexistent_file_returns_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nonexistent_path = temp_dir.path().join("does_not_exist.json");
    let result = WindowsState::load_from_path(&nonexistent_path);
    assert!(
        result.is_err(),
        "Loading non-existent file should return an error"
    );
}

#[test]
fn test_state_load_invalid_json_returns_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let invalid_path = temp_state_path(&temp_dir);
    fs::write(&invalid_path, "{ this is not valid json }").expect("Failed to write invalid JSON");
    let result = WindowsState::load_from_path(&invalid_path);
    assert!(
        result.is_err(),
        "Loading invalid JSON should return an error"
    );
}

#[test]
fn test_state_json_structure_validation() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let state = WindowsState {
        windows: vec![WindowState {
            tabs: vec![create_file_tab_modified()],
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
    let json_content = fs::read_to_string(&state_path).expect("Failed to read state file");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_content).expect("JSON should be valid");
    assert!(parsed["windows"].is_array(), "Should have windows array");
    assert!(parsed["windows"][0].is_object(), "Window should be object");
    assert!(
        parsed["windows"][0]["tabs"].is_array(),
        "Should have tabs array"
    );
    assert!(
        parsed["windows"][0]["tabs"][0]["title"].is_string(),
        "Tab title should be string"
    );
    assert!(
        parsed["windows"][0]["window_bounds"].is_object(),
        "Should have window_bounds object"
    );
    assert_eq!(
        parsed["windows"][0]["window_bounds"]["state"].as_str(),
        Some("Windowed")
    );
    assert_eq!(
        parsed["windows"][0]["window_bounds"]["x"].as_f64(),
        Some(100.0)
    );
}

#[test]
fn test_state_preserves_window_order() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let mut windows = Vec::new();
    for i in 0..5 {
        windows.push(WindowState {
            tabs: vec![TabState {
                title: format!("Window {} Marker", i),
                file_path: None,
                content: Some(format!("This is window number {}", i)),
                last_saved: None,
            }],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds {
                state: "Windowed".to_string(),
                x: (i as f32) * 100.0,
                y: (i as f32) * 100.0,
                width: 800.0,
                height: 600.0,
                display_id: Some(i as u32),
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
            format!("Window {} Marker", i),
            "Window order should be preserved"
        );
        assert_eq!(
            loaded.windows[i].window_bounds.x,
            (i as f32) * 100.0,
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
                tabs: vec![
                    create_file_tab_unmodified(),
                    create_file_tab_modified(),
                    create_unsaved_tab(),
                ],
                active_tab_index: Some(0), // First tab active
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                tabs: vec![
                    create_file_tab_unmodified(),
                    create_file_tab_modified(),
                    create_unsaved_tab(),
                ],
                active_tab_index: Some(1), // Second tab active
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                tabs: vec![
                    create_file_tab_unmodified(),
                    create_file_tab_modified(),
                    create_unsaved_tab(),
                ],
                active_tab_index: Some(2), // Third tab active
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                tabs: vec![
                    create_file_tab_unmodified(),
                    create_file_tab_modified(),
                    create_unsaved_tab(),
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
                tabs: vec![create_unsaved_tab()],
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
                tabs: vec![create_unsaved_tab()],
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
                tabs: vec![create_unsaved_tab()],
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
    assert_eq!(loaded.windows[0].window_bounds.x, 100.0);
    assert_eq!(loaded.windows[1].window_bounds.x, 1920.0);
    assert_eq!(loaded.windows[2].window_bounds.x, 3840.0);
}

#[test]
fn test_state_mixed_window_and_tab_counts() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let state_path = temp_state_path(&temp_dir);
    let original = WindowsState {
        windows: vec![
            WindowState {
                tabs: vec![create_file_tab_unmodified()], // 1 tab
                active_tab_index: Some(0),
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                tabs: vec![
                    create_file_tab_unmodified(),
                    create_file_tab_modified(),
                    create_unsaved_tab(),
                    create_file_tab_unmodified(),
                    create_file_tab_modified(),
                ], // 5 tabs
                active_tab_index: Some(2),
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                tabs: vec![], // 0 tabs
                active_tab_index: None,
                window_bounds: SerializedWindowBounds::default(),
            },
            WindowState {
                tabs: vec![create_file_tab_unmodified(), create_unsaved_tab()], // 2 tabs
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
