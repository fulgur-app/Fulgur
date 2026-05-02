/// Integration tests for concurrent file access
///
/// These tests verify that settings and state files can be safely written
/// by multiple threads concurrently without corruption, using atomic file writes.
use fulgur::fulgur::settings::Settings;
use fulgur::fulgur::state_persistence::{
    SerializedWindowBounds, TabState, WindowState, WindowsState,
};
use std::path::PathBuf;
use std::sync::Arc;
use std::thread;
use tempfile::TempDir;

/// Build an OS-agnostic temporary path used by test-only tab state values.
///
/// ### Parameters
/// - `name`: The file name to append to the platform temp directory.
///
/// ### Returns
/// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests.
fn temp_test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

/// Test concurrent writes to settings file don't corrupt data
///
/// Simulates multiple windows trying to save settings simultaneously.
/// Atomic file writes prevent corrupted JSON.
#[test]
fn test_settings_concurrent_writes_no_corruption() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");
    let settings_path = Arc::new(settings_path);
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = Arc::clone(&settings_path);
            thread::spawn(move || {
                let mut settings = Settings::new();
                // Each thread sets a different font size to make writes distinguishable
                settings.editor_settings.font_size = 10.0 + i as f32;
                settings.save_to_path(&path).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let final_settings = Settings::load_from_path(&settings_path).unwrap();
    assert!(final_settings.editor_settings.font_size >= 10.0);
    assert!(final_settings.editor_settings.font_size <= 19.0);
}

/// Test concurrent writes to state file don't corrupt data
///
/// Simulates multiple windows trying to save state simultaneously.
#[test]
fn test_state_concurrent_writes_no_corruption() {
    let temp_dir = TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.json");
    let state_path = Arc::new(state_path);
    let handles: Vec<_> = (0..10)
        .map(|i| {
            let path = Arc::clone(&state_path);
            thread::spawn(move || {
                let mut state = WindowsState { windows: vec![] };
                let window = WindowState {
                    tabs: vec![TabState {
                        title: format!("Thread {i}"),
                        file_path: None,
                        content: Some(format!("Content from thread {i}")),
                        last_saved: None,
                        remote: None,
                    }],
                    active_tab_index: Some(0),
                    window_bounds: SerializedWindowBounds::default(),
                };
                state.windows.push(window);
                state.save_to_path(&path).unwrap();
            })
        })
        .collect();
    for handle in handles {
        handle.join().unwrap();
    }
    let final_state = WindowsState::load_from_path(&state_path).unwrap();
    assert_eq!(final_state.windows.len(), 1);
    let title = &final_state.windows[0].tabs[0].title;
    assert!(title.starts_with("Thread "));
}

/// Test mixed concurrent reads and writes
///
/// Verifies that reading while another thread is writing still produces valid data.
#[test]
fn test_settings_concurrent_read_write() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");
    let mut initial_settings = Settings::new();
    initial_settings.editor_settings.font_size = 14.0;
    initial_settings.save_to_path(&settings_path).unwrap();
    let settings_path = Arc::new(settings_path);
    let mut handles = vec![];
    for i in 0..5 {
        let path = Arc::clone(&settings_path);
        handles.push(thread::spawn(move || {
            let mut settings = Settings::new();
            settings.editor_settings.font_size = 20.0 + i as f32;
            settings.save_to_path(&path).unwrap();
        }));
    }
    for _ in 0..5 {
        let path = Arc::clone(&settings_path);
        handles.push(thread::spawn(move || {
            // Try to read - should always get valid settings
            let settings = Settings::load_from_path(&path);
            assert!(
                settings.is_ok(),
                "Failed to load settings during concurrent access"
            );
        }));
    }
    for handle in handles {
        handle.join().unwrap();
    }
    let final_settings = Settings::load_from_path(&settings_path).unwrap();
    assert!(final_settings.editor_settings.font_size >= 14.0);
}

/// Test sequential writes from the same thread
///
/// Verifies that multiple sequential writes from the same thread work correctly.
#[test]
fn test_settings_sequential_writes_same_thread() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");
    for i in 0..10 {
        let mut settings = Settings::new();
        settings.editor_settings.font_size = 10.0 + i as f32;
        settings.save_to_path(&settings_path).unwrap();
    }
    let final_settings = Settings::load_from_path(&settings_path).unwrap();
    assert_eq!(final_settings.editor_settings.font_size, 19.0);
}

/// Test sequential writes succeed
///
/// Verifies that consecutive writes produce valid data.
#[test]
fn test_settings_lock_released_after_write() {
    let temp_dir = TempDir::new().unwrap();
    let settings_path = temp_dir.path().join("settings.json");
    let mut settings = Settings::new();
    settings.editor_settings.font_size = 14.0;
    settings.save_to_path(&settings_path).unwrap();
    let mut settings = Settings::new();
    settings.editor_settings.font_size = 16.0;
    settings.save_to_path(&settings_path).unwrap();
    let loaded = Settings::load_from_path(&settings_path).unwrap();
    assert_eq!(loaded.editor_settings.font_size, 16.0);
}

/// Test concurrent writes with larger data
///
/// Verifies that atomic writes work correctly with more realistic data sizes.
#[test]
fn test_state_concurrent_writes_large_data() {
    let temp_dir = TempDir::new().unwrap();
    let state_path = temp_dir.path().join("state.json");
    let state_path = Arc::new(state_path);

    // Spawn threads that write larger state objects
    let handles: Vec<_> = (0..5)
        .map(|i| {
            let path = Arc::clone(&state_path);
            thread::spawn(move || {
                let mut state = WindowsState { windows: vec![] };

                // Create multiple windows with multiple tabs
                for w in 0..3 {
                    let mut tabs = vec![];
                    for t in 0..5 {
                        tabs.push(TabState {
                            title: format!("Thread {i} Window {w} Tab {t}"),
                            file_path: Some(temp_test_path(&format!("thread_{i}_file_{t}.txt"))),
                            content: Some("x".repeat(1000)), // 1KB of content per tab
                            last_saved: Some("2026-02-13T10:00:00Z".to_string()),
                            remote: None,
                        });
                    }

                    state.windows.push(WindowState {
                        tabs,
                        active_tab_index: Some(0),
                        window_bounds: SerializedWindowBounds {
                            state: "Windowed".to_string(),
                            x: (i * 100) as f32,
                            y: (i * 100) as f32,
                            width: 1200.0,
                            height: 800.0,
                            display_id: Some(i as u32),
                        },
                    });
                }

                state.save_to_path(&path).unwrap();
            })
        })
        .collect();

    for handle in handles {
        handle.join().unwrap();
    }
    let final_state = WindowsState::load_from_path(&state_path).unwrap();
    assert_eq!(final_state.windows.len(), 3);
    for window in &final_state.windows {
        assert_eq!(window.tabs.len(), 5);
    }
}
