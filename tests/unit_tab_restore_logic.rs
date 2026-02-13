//! Unit tests for tab restoration decision logic
//!
//! These tests verify the `determine_tab_restore_strategy()` function works correctly
//! for all decision paths when restoring tabs from saved state.

use std::path::PathBuf;

use fulgur::fulgur::state_operations::{TabRestoreDecision, determine_tab_restore_strategy};

fn test_path() -> PathBuf {
    PathBuf::from("/tmp/test.txt")
}

fn test_content() -> String {
    "test content".to_string()
}

// ISO 8601 timestamp strings for testing
fn now() -> String {
    "2024-01-15T12:00:00Z".to_string()
}

fn past() -> String {
    "2024-01-15T11:00:00Z".to_string() // 1 hour before "now"
}

// Case 1: Has both path and content (modified file)

#[test]
fn test_restore_modified_file_newer_than_saved() {
    // File was modified externally after we saved - should load from file
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(past()), // saved 1 hour ago
        true,         // file exists
        Some(now()),  // file modified now (newer)
        true,         // can read file
    );
    assert_eq!(
        decision,
        TabRestoreDecision::LoadFromFile { path: test_path() }
    );
}

#[test]
fn test_restore_modified_file_older_than_saved() {
    // File wasn't modified externally - should preserve unsaved changes
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(now()),  // saved now
        true,         // file exists
        Some(past()), // file modified 1 hour ago (older)
        true,         // can read file
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentWithPath {
            path: test_path(),
            content: test_content()
        }
    );
}

#[test]
fn test_restore_modified_file_newer_but_cannot_read() {
    // File is newer but can't read it - use saved content
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(past()),
        true, // file exists
        Some(now()),
        false, // CANNOT read file
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentWithPath {
            path: test_path(),
            content: test_content()
        }
    );
}

#[test]
fn test_restore_modified_file_no_timestamp_info() {
    // No timestamp info - prefer saved content (safer)
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        None, // no last_saved timestamp
        true, // file exists
        None, // no file modified time
        true,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentWithPath {
            path: test_path(),
            content: test_content()
        }
    );
}

#[test]
fn test_restore_modified_file_has_saved_time_but_no_file_time() {
    // Has saved time but can't get file time - use saved content
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(now()), // has last_saved
        true,        // file exists
        None,        // no file modified time
        true,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentWithPath {
            path: test_path(),
            content: test_content()
        }
    );
}

#[test]
fn test_restore_modified_file_has_file_time_but_no_saved_time() {
    // Has file time but no saved time - use saved content
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        None,        // no last_saved
        true,        // file exists
        Some(now()), // has file time
        true,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentWithPath {
            path: test_path(),
            content: test_content()
        }
    );
}

#[test]
fn test_restore_modified_file_deleted() {
    // File was deleted - use saved content without path
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(past()),
        false, // file DOESN'T exist
        None,
        false,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentNoPath {
            content: test_content()
        }
    );
}

// Case 2: Has path only (unmodified file)

#[test]
fn test_restore_unmodified_file_exists() {
    // Clean file that exists - load from file
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        None, // no saved content
        None,
        true, // file exists
        Some(now()),
        true, // can read
    );
    assert_eq!(
        decision,
        TabRestoreDecision::LoadFromFile { path: test_path() }
    );
}

#[test]
fn test_restore_unmodified_file_deleted() {
    // Clean file that was deleted - skip
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        None,
        None,
        false, // file doesn't exist
        None,
        false,
    );
    assert_eq!(decision, TabRestoreDecision::Skip);
}

#[test]
fn test_restore_unmodified_file_cannot_read() {
    // File exists but can't read - skip
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        None,
        None,
        true, // file exists
        Some(now()),
        false, // CANNOT read
    );
    assert_eq!(decision, TabRestoreDecision::Skip);
}

// Case 3: Has content only (unsaved tab)

#[test]
fn test_restore_unsaved_tab() {
    // Unsaved tab - use content without path
    let decision = determine_tab_restore_strategy(
        None, // no path
        Some(test_content()),
        None,
        false,
        None,
        false,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentNoPath {
            content: test_content()
        }
    );
}

// Case 4: Has neither (invalid state)

#[test]
fn test_restore_nothing() {
    // No path, no content - skip
    let decision = determine_tab_restore_strategy(None, None, None, false, None, false);
    assert_eq!(decision, TabRestoreDecision::Skip);
}

#[test]
fn test_restore_file_same_timestamp_as_saved() {
    // File and saved have same timestamp - prefer saved content (no external change)
    let timestamp = now();
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(timestamp.clone()),
        true,
        Some(timestamp), // exactly same time
        true,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::UseSavedContentWithPath {
            path: test_path(),
            content: test_content()
        }
    );
}

#[test]
fn test_restore_file_second_newer() {
    // File is 1 second newer - should detect as newer and load from file
    let saved_time = "2024-01-15T12:00:00Z".to_string();
    let file_time = "2024-01-15T12:00:01Z".to_string(); // 1 second later
    let decision = determine_tab_restore_strategy(
        Some(test_path()),
        Some(test_content()),
        Some(saved_time),
        true,
        Some(file_time),
        true,
    );
    assert_eq!(
        decision,
        TabRestoreDecision::LoadFromFile { path: test_path() }
    );
}

#[test]
fn test_restore_multiple_paths() {
    // Test with different path values to ensure path is preserved correctly
    let paths = vec![
        PathBuf::from("/home/user/file.txt"),
        PathBuf::from("C:\\Users\\user\\file.txt"),
        PathBuf::from("relative/path/file.txt"),
    ];

    for path in paths {
        let decision =
            determine_tab_restore_strategy(Some(path.clone()), None, None, true, Some(now()), true);
        assert_eq!(
            decision,
            TabRestoreDecision::LoadFromFile { path: path.clone() }
        );
    }
}
