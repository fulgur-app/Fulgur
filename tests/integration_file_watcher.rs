//! Integration tests for File Watching functionality
//!
//! These tests verify that the `FileWatcher` can detect real file system changes
//! and properly handle events. They run in CI/CD environments using temporary
//! directories for isolation.

use std::fs;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;
use tempfile::TempDir;

// Import from the main crate
use fulgur::fulgur::files::file_watcher::{FileWatchEvent, FileWatcher};
use futures::channel::mpsc::Receiver;

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
fn paths_equal(path1: &Path, path2: &Path) -> bool {
    let canon1 = path1.canonicalize().unwrap_or_else(|_| path1.to_path_buf());
    let canon2 = path2.canonicalize().unwrap_or_else(|_| path2.to_path_buf());
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
    rx: &mut Receiver<FileWatchEvent>,
    timeout_ms: u64,
    predicate: F,
) -> Option<FileWatchEvent>
where
    F: Fn(&FileWatchEvent) -> bool,
{
    wait_for_event_ticking(rx, timeout_ms, predicate, || {})
}

/// Wait for a specific event from the file watcher, running a callback before each poll
///
/// ### Arguments
/// - `rx`: The receiver to wait for events from
/// - `timeout_ms`: Maximum time to wait in milliseconds
/// - `predicate`: Function to test if the received event matches expectations
/// - `on_tick`: Callback run before each poll, e.g. to expire a pending rename
///
/// ### Returns
/// - `Some(FileWatchEvent)`: The matching event if found within timeout
/// - `None`: If timeout was reached or no matching event found
fn wait_for_event_ticking<F, T>(
    rx: &mut Receiver<FileWatchEvent>,
    timeout_ms: u64,
    predicate: F,
    mut on_tick: T,
) -> Option<FileWatchEvent>
where
    F: Fn(&FileWatchEvent) -> bool,
    T: FnMut(),
{
    let start = std::time::Instant::now();
    let timeout = Duration::from_millis(timeout_ms);
    let mut events_seen = Vec::new();

    while start.elapsed() < timeout {
        on_tick();
        if let Ok(event) = rx.try_recv() {
            // Debug: collect events seen
            events_seen.push(format!("{event:?}"));

            if predicate(&event) {
                return Some(event);
            }
        }
        thread::sleep(Duration::from_millis(50));
    }
    if !events_seen.is_empty() {
        eprintln!("Events received but didn't match: {events_seen:?}");
    }
    None
}

/// Discard every event already queued on the receiver
///
/// ### Arguments
/// - `rx`: The receiver to drain
fn drain_events(rx: &mut Receiver<FileWatchEvent>) {
    while rx.try_recv().is_ok() {}
}

/// Tell whether a watch event concerns the given path
///
/// ### Arguments
/// - `event`: The event to inspect
/// - `path`: The path to look for
///
/// ### Returns
/// - `bool`: True if the event names the path, as either side of a rename included
fn event_concerns(event: &FileWatchEvent, path: &Path) -> bool {
    match event {
        FileWatchEvent::Modified(event_path) | FileWatchEvent::Deleted(event_path) => {
            paths_equal(event_path, path)
        }
        FileWatchEvent::Renamed { from, to } => paths_equal(from, path) || paths_equal(to, path),
        FileWatchEvent::Error(_) => false,
    }
}

// ============================================================================
// FileWatcher Tests
// ============================================================================

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
    let result = watcher.watch_file(&file_path);
    assert!(result.is_ok(), "Should successfully watch file");
    let result2 = watcher.watch_file(&file_path);
    assert!(result2.is_ok(), "Watching same file again should succeed");
}

#[test]
fn test_unwatch_file() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, mut rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(&file_path)
        .expect("Failed to watch file");
    thread::sleep(Duration::from_millis(500));
    watcher.unwatch_file(&file_path);
    // Unwatching again should be safe (idempotent)
    watcher.unwatch_file(&file_path);

    // Events queued before the unwatch (file creation, inotify IN_IGNORED) are not under test
    thread::sleep(Duration::from_millis(500));
    drain_events(&mut rx);

    fs::write(&file_path, "modified after unwatch").expect("Failed to modify file");
    let event = wait_for_event(&mut rx, 1000, |event| event_concerns(event, &file_path));
    assert!(
        event.is_none(),
        "Should not receive events for an unwatched file, got {event:?}"
    );
}

#[test]
fn test_detect_file_modification() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let (mut watcher, mut rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(&file_path)
        .expect("Failed to watch file");
    // Give watcher more time to initialize (file watchers can be slow)
    thread::sleep(Duration::from_millis(500));
    fs::write(&file_path, "modified content").expect("Failed to modify file");
    let file_path_clone = file_path.clone();
    let event = wait_for_event(
        &mut rx,
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
    let (mut watcher, mut rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(&file_path)
        .expect("Failed to watch file");
    // Give watcher more time to initialize
    thread::sleep(Duration::from_millis(500));
    fs::remove_file(&file_path).expect("Failed to delete file");
    let file_path_clone = file_path.clone();
    let event = wait_for_event(
        &mut rx,
        5000,
        |event| matches!(event, FileWatchEvent::Deleted(path) if paths_equal(path, &file_path_clone)),
    );
    assert!(
        event.is_some(),
        "Should receive Deleted event for file removal"
    );
}

/// A watched file renamed away is reported as deleted at its old path.
#[test]
fn test_rename_away_reports_old_path_deleted() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let from_path = create_test_file(&temp_dir, "old_name.txt", "content");
    let to_path = temp_dir.path().join("new_name.txt");
    let (mut watcher, mut rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(&from_path)
        .expect("Failed to watch file");
    // Give watcher more time to initialize
    thread::sleep(Duration::from_millis(500));
    fs::rename(&from_path, &to_path).expect("Failed to rename file");
    let event = wait_for_event_ticking(
        &mut rx,
        5000,
        |event| matches!(event, FileWatchEvent::Deleted(path) if paths_equal(path, &from_path)),
        || watcher.flush_expired_pending_rename(),
    );
    assert!(
        event.is_some(),
        "Should receive Deleted for the old path after a rename away"
    );
}

#[test]
fn test_watch_multiple_files() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file1 = create_test_file(&temp_dir, "file1.txt", "content1");
    let file2 = create_test_file(&temp_dir, "file2.txt", "content2");
    let file3 = create_test_file(&temp_dir, "file3.txt", "content3");
    let (mut watcher, mut rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher.watch_file(&file1).expect("Failed to watch file1");
    watcher.watch_file(&file2).expect("Failed to watch file2");
    watcher.watch_file(&file3).expect("Failed to watch file3");
    // Give watcher more time to initialize
    thread::sleep(Duration::from_millis(500));
    fs::write(&file2, "modified content2").expect("Failed to modify file2");
    let file2_clone = file2.clone();
    let event = wait_for_event(
        &mut rx,
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
    let (mut watcher, mut rx) = FileWatcher::new();
    watcher.start().expect("Failed to start watcher");
    watcher
        .watch_file(&file_path)
        .expect("Failed to watch file");
    // Give watcher time to initialize
    thread::sleep(Duration::from_millis(100));
    watcher.stop();

    // Drain any pending events from before the stop
    // (these could be from the initial watch setup)
    thread::sleep(Duration::from_millis(500));
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

/// Dropping the watcher tears down the OS watch and its callback, releasing every
/// event sender, so the receiver observes the channel closing.
#[test]
fn test_watcher_drop_closes_event_channel() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let file_path = create_test_file(&temp_dir, "test.txt", "initial content");
    let mut rx = {
        let (mut watcher, rx) = FileWatcher::new();
        watcher.start().expect("Failed to start watcher");
        watcher
            .watch_file(&file_path)
            .expect("Failed to watch file");
        thread::sleep(Duration::from_millis(100));
        rx
    };
    fs::write(&file_path, "modified after drop").expect("Failed to modify file");

    // Events queued before the drop may still be delivered; the backend's event thread
    // may also take a moment to exit and release its sender.
    let start = std::time::Instant::now();
    let closed = loop {
        match rx.try_recv() {
            Ok(_) => {}
            Err(e) if e.is_closed() => break true,
            Err(_) if start.elapsed() >= Duration::from_secs(5) => break false,
            Err(_) => thread::sleep(Duration::from_millis(50)),
        }
    };
    assert!(
        closed,
        "Dropping the watcher should close the event channel"
    );
}
