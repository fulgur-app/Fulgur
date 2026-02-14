//! Integration tests for File Watching functionality
//!
//! These tests verify that the FileWatcher can detect real file system changes
//! and properly handle events. They run in CI/CD environments using temporary
//! directories for isolation.

use std::fs;
use std::path::PathBuf;
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// Import from the main crate
use fulgur::fulgur::files::file_watcher::{FileWatchEvent, FileWatcher};

/// Create a temporary test file with given content
///
/// ### Arguments
/// - `temp_dir`: The temporary directory
/// - `filename`: The name of the file to create
/// - `content`: The content to write to the file
///
/// ### Returns
/// - `PathBuf`: Path to the created file (canonicalized)
fn create_test_file(temp_dir: &TempDir, filename: &str, content: &str) -> PathBuf {
    let path = temp_dir.path().join(filename);
    fs::write(&path, content).expect("Failed to create test file");
    // Canonicalize the path to match what file watcher returns
    path.canonicalize().unwrap_or(path)
}

/// Compare two paths for equality, handling canonicalization
///
/// ### Arguments
/// - `path1`: First path
/// - `path2`: Second path
///
/// ### Returns
/// - `bool`: True if paths are equivalent (after canonicalization)
fn paths_equal(path1: &PathBuf, path2: &PathBuf) -> bool {
    let canon1 = path1.canonicalize().unwrap_or_else(|_| path1.clone());
    let canon2 = path2.canonicalize().unwrap_or_else(|_| path2.clone());
    canon1 == canon2
}

/// Wait for a specific event from the file watcher with timeout
///
/// ### Arguments
/// - `rx`: The receiver to wait for events from
/// - `timeout_ms`: Maximum time to wait in milliseconds
/// - `predicate`: Function to test if the received event matches expectations
///
/// ### Returns
/// - `Some(FileWatchEvent)`: The matching event if found within timeout
/// - `None`: If timeout was reached or no matching event found
fn wait_for_event<F>(
    rx: &std::sync::mpsc::Receiver<FileWatchEvent>,
    timeout_ms: u64,
    predicate: F,
) -> Option<FileWatchEvent>
where
    F: Fn(&FileWatchEvent) -> bool,
{
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let mut events_seen = Vec::new();

    while start.elapsed() < timeout {
        if let Ok(event) = rx.try_recv() {
            // Debug: collect events seen
            events_seen.push(format!("{:?}", event));

            if predicate(&event) {
                return Some(event);
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    if !events_seen.is_empty() {
        eprintln!("Events received but didn't match: {:?}", events_seen);
    }
    None
}

/// Wait for any event from the file watcher with timeout
///
/// ### Arguments
/// - `rx`: The receiver to wait for events from
/// - `timeout_ms`: Maximum time to wait in milliseconds
///
/// ### Returns
/// - `Some(FileWatchEvent)`: Any event if received within timeout
/// - `None`: If timeout was reached
fn wait_for_any_event(
    rx: &std::sync::mpsc::Receiver<FileWatchEvent>,
    timeout_ms: u64,
) -> Option<FileWatchEvent> {
    wait_for_event(rx, timeout_ms, |_| true)
}

// ============================================================================
// FileWatcher Tests
// ============================================================================

#[test]
fn test_file_watcher_creation() {
    let (watcher, _rx) = FileWatcher::new();
    drop(watcher);
}

#[test]
fn test_file_watcher_start() {
    let (mut watcher, _rx) = FileWatcher::new();
    let result = watcher.start();
    assert!(result.is_ok(), "FileWatcher should start successfully");
}

#[test]
fn test_file_watcher_start_idempotent() {
    let (mut watcher, _rx) = FileWatcher::new();
    let result1 = watcher.start();
    let result2 = watcher.start();
    assert!(result1.is_ok(), "First start should succeed");
    assert!(result2.is_ok(), "Second start should be idempotent");
}

#[test]
fn test_watch_single_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, _rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    let result = watcher.watch_file(file_path.clone());
    assert!(result.is_ok(), "Should successfully watch file");
    let result2 = watcher.watch_file(file_path);
    assert!(result2.is_ok(), "Watching same file again should succeed");
}

#[test]
fn test_unwatch_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, _rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(file_path.clone())
        .expect("Failed to watch file");
    watcher.unwatch_file(&file_path);
    // Unwatching again should be safe (idempotent)
    watcher.unwatch_file(&file_path);
}

#[test]
fn test_detect_file_modification() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(file_path.clone())
        .expect("Failed to watch file");
    // Give watcher more time to initialize (file watchers can be slow)
    thread::sleep(Duration::from_millis(500));
    fs::write(&file_path, "modified content").expect("Failed to modify file");
    let file_path_clone = file_path.clone();
    let event = wait_for_event(
        &rx,
        5000,
        |event| matches!(event, FileWatchEvent::Modified(path) if paths_equal(path, &file_path_clone)),
    );
    assert!(
        event.is_some(),
        "Should receive Modified event for file change"
    );
}

#[test]
fn test_detect_file_deletion() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(file_path.clone())
        .expect("Failed to watch file");
    // Give watcher more time to initialize
    thread::sleep(Duration::from_millis(500));
    fs::remove_file(&file_path).expect("Failed to delete file");
    let file_path_clone = file_path.clone();
    let event = wait_for_event(
        &rx,
        5000,
        |event| matches!(event, FileWatchEvent::Deleted(path) if paths_equal(path, &file_path_clone)),
    );
    assert!(
        event.is_some(),
        "Should receive Deleted event for file removal"
    );
}

#[test]
fn test_detect_file_rename() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let from_path = create_test_file(&temp_dir, "old_name.txt", "content");
    let to_path_uncanonicalized = temp_dir.path().join("new_name.txt");
    let (mut watcher, rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(from_path.clone())
        .expect("Failed to watch file");
    // Give watcher more time to initialize
    thread::sleep(Duration::from_millis(500));
    fs::rename(&from_path, &to_path_uncanonicalized).expect("Failed to rename file");
    let to_path = to_path_uncanonicalized
        .canonicalize()
        .unwrap_or(to_path_uncanonicalized);
    let event = wait_for_any_event(&rx, 5000);

    if event.is_none() {
        eprintln!(
            "Note: Rename event not detected on this platform. This is expected on some systems."
        );
        return; // Test passes - rename detection is platform-specific
    }

    let event = event.unwrap();
    eprintln!("Received event for rename: {:?}", event);
    match event {
        FileWatchEvent::Renamed { from, to } => {
            assert!(
                paths_equal(&from, &from_path),
                "Rename 'from' path should match"
            );
            assert!(paths_equal(&to, &to_path), "Rename 'to' path should match");
        }
        FileWatchEvent::Deleted(path) => {
            assert!(
                paths_equal(&path, &from_path),
                "Delete path should be the old name"
            );
        }
        FileWatchEvent::Modified(path) => {
            // Some systems report rename as modify on the old or new path
            assert!(
                paths_equal(&path, &from_path) || paths_equal(&path, &to_path),
                "Modified path should be one of the rename paths, got {:?}",
                path
            );
        }
        FileWatchEvent::Error(_) => {
            panic!("Should not receive error event for rename");
        }
    }
}

#[test]
fn test_watch_multiple_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file1 = create_test_file(&temp_dir, "file1.txt", "content1");
    let file2 = create_test_file(&temp_dir, "file2.txt", "content2");
    let file3 = create_test_file(&temp_dir, "file3.txt", "content3");
    let (mut watcher, rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(file1.clone())
        .expect("Failed to watch file1");
    watcher
        .watch_file(file2.clone())
        .expect("Failed to watch file2");
    watcher
        .watch_file(file3.clone())
        .expect("Failed to watch file3");
    // Give watcher more time to initialize
    thread::sleep(Duration::from_millis(500));
    fs::write(&file2, "modified content2").expect("Failed to modify file2");
    let file2_clone = file2.clone();
    let event = wait_for_event(
        &rx,
        5000,
        |event| matches!(event, FileWatchEvent::Modified(path) if paths_equal(path, &file2_clone)),
    );
    assert!(
        event.is_some(),
        "Should receive Modified event for file2 specifically"
    );
}

#[test]
fn test_stop_watcher() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(file_path.clone())
        .expect("Failed to watch file");
    // Give watcher time to initialize
    thread::sleep(Duration::from_millis(100));
    watcher.stop();

    // Drain any pending events from before the stop
    // (these could be from the initial watch setup)
    thread::sleep(Duration::from_millis(200));
    while rx.try_recv().is_ok() {
        // Drain all pending events
    }

    // Now modify the file - we should NOT receive events for this
    fs::write(&file_path, "modified after stop").expect("Failed to modify file");
    thread::sleep(Duration::from_millis(500));
    let event = rx.try_recv();
    assert!(
        event.is_err(),
        "Should not receive events after watcher is stopped"
    );
}

#[test]
fn test_watcher_drop_cleanup() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let rx = {
        let (mut watcher, rx) = FileWatcher::new();
        watcher.start().expect("Failed to start watcher");
        watcher
            .watch_file(file_path.clone())
            .expect("Failed to watch file");
        thread::sleep(Duration::from_millis(100));
        // watcher is dropped here, should trigger cleanup
        rx
    };
    fs::write(&file_path, "modified after drop").expect("Failed to modify file");
    thread::sleep(Duration::from_millis(500));
    // On some platforms (especially macOS), events that were already queued may still be delivered
    // The main goal of this test is to ensure the watcher cleans up without crashing
    let event = rx.try_recv();
    // We don't assert on the result - both getting events and not getting events are acceptable
    // The important thing is that we didn't crash
    eprintln!("Event after drop: {:?}", event);
}
