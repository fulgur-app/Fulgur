use super::{
    assert_recovery_content, configured_tab_states, restore_and_snapshot, startup_snapshot,
};
use crate::fulgur::{
    Fulgur,
    editor_tab::TabLocation,
    state::persistence::{TabContent, TabState, file_modified_system_time, format_modified_time},
};
use crate::test_support::setup_fulgur_with_root as setup_fulgur;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::{
    fs,
    path::Path,
    time::{Duration, SystemTime},
};
use tempfile::TempDir;

/// Write a file and pin its modification time, so tests do not depend on clock resolution.
///
/// ### Arguments
/// - `path`: The file to write
/// - `text`: The file content
/// - `unix_seconds`: The modification time to set, in seconds since the Unix epoch
///
/// ### Returns
/// - `SystemTime`: The modification time now carried by the file
fn write_with_mtime(path: &Path, text: &str, unix_seconds: u64) -> SystemTime {
    fs::write(path, text).expect("write test file");
    let modified = SystemTime::UNIX_EPOCH + Duration::from_secs(unix_seconds);
    fs::File::options()
        .write(true)
        .open(path)
        .and_then(|file| file.set_modified(modified))
        .expect("set test file mtime");
    modified
}

/// Open a dirty tab on a file, change the file behind it, then snapshot the window.
///
/// ### Arguments
/// - `fulgur`: The application entity whose first tab is configured
/// - `cx`: The visual test context containing the application window
/// - `path`: The file backing the tab
/// - `before_snapshot`: Called after the external change, before the snapshot is taken
///
/// ### Returns
/// - `(TabState, SystemTime)`: The persisted tab and the mtime of the version the edits are based on
fn snapshot_dirty_tab_after_external_change(
    fulgur: &Entity<Fulgur>,
    cx: &mut VisualTestContext,
    path: &Path,
    before_snapshot: impl FnOnce(&mut Fulgur, &mut gpui_kit::Context<Fulgur>),
) -> (TabState, SystemTime) {
    let opened_version = write_with_mtime(path, "on disk", 1_000_000_000);
    configured_tab_states(
        fulgur,
        cx,
        TabLocation::Local(path.to_path_buf()),
        true,
        "local edits",
        |editor_tab| {
            editor_tab.title = "notes.txt".into();
            editor_tab.set_original_content_from_str("on disk");
            editor_tab.modified = true;
            editor_tab.file_last_modified = file_modified_system_time(path);
        },
    );
    write_with_mtime(path, "changed externally", 1_100_000_000);
    cx.update(|_, cx| fulgur.update(cx, before_snapshot));
    let snapshot = fulgur.read_with(cx, Fulgur::build_window_state_without_bounds);
    let persisted = snapshot.tabs.into_iter().next().expect("expected a tab");
    (persisted, opened_version)
}

/// Whether the window queued a file conflict for a path.
///
/// ### Arguments
/// - `fulgur`: The application entity to inspect
/// - `cx`: The visual test context containing the application window
/// - `path`: The file path to look up
///
/// ### Returns
/// - `bool`: `true` when a conflict is pending for `path`
fn conflict_queued_for(fulgur: &Entity<Fulgur>, cx: &VisualTestContext, path: &Path) -> bool {
    fulgur.read_with(cx, |this, _| {
        this.file_watch_state.pending_conflicts.contains_key(path)
    })
}

#[gpui_kit::test]
fn restored_local_recovery_survives_newer_file_and_queues_conflict(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("touched.txt");
    fs::write(&path, "changed on disk").expect("write saved file");
    let restored = TabState {
        tab_id: 9,
        title: "touched.txt".to_string(),
        file_path: Some(path.clone()),
        content: Some(TabContent::from("recovered local edits")),
        last_saved: Some("2000-01-01T00:00:00Z".to_string()),
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let snapshot = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));

    assert_recovery_content(&snapshot.tabs[0], "recovered local edits");
    let conflict_queued = fulgur.read_with(&visual_cx, |this, _| {
        this.file_watch_state.pending_conflicts.contains_key(&path)
    });
    assert!(
        conflict_queued,
        "a file changed on disk after the edits were persisted must raise a conflict"
    );
    assert_eq!(
        snapshot.tabs[0].last_saved.as_deref(),
        Some("2000-01-01T00:00:00Z"),
        "an unresolved conflict must keep its persisted baseline across restarts"
    );
}

#[gpui_kit::test]
fn in_session_external_change_under_dirty_tab_raises_conflict_on_restore(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("notes.txt");
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let (persisted, opened_version) =
        snapshot_dirty_tab_after_external_change(&fulgur, &mut visual_cx, &path, |_, _| {});

    assert_eq!(
        persisted.last_saved,
        format_modified_time(opened_version),
        "the snapshot must record the version the edits are based on, not the current disk mtime"
    );
    let restored = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(persisted));
    assert_recovery_content(&restored.tabs[0], "local edits");
    assert!(
        conflict_queued_for(&fulgur, &visual_cx, &path),
        "a disk change the user never resolved must raise a conflict after the restart"
    );
}

#[gpui_kit::test]
fn keeping_local_changes_accepts_the_disk_version_as_baseline(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("notes.txt");
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let (persisted, _) =
        snapshot_dirty_tab_after_external_change(&fulgur, &mut visual_cx, &path, |this, cx| {
            let tab_id = this.tabs[0].read(cx).id();
            this.keep_local_changes_over_disk(tab_id, &path, cx);
        });

    assert_eq!(
        persisted.last_saved,
        file_modified_system_time(&path).and_then(format_modified_time)
    );
    let restored = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(persisted));
    assert_recovery_content(&restored.tabs[0], "local edits");
    assert!(
        !conflict_queued_for(&fulgur, &visual_cx, &path),
        "a conflict the user resolved by keeping local changes must not come back"
    );
}
