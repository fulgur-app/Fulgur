//! Tests for the `SQLite` session-state store.

use super::StateDb;
use crate::fulgur::state::persistence::{
    SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
};

/// Build a tab state with a given identity and title.
fn tab(tab_id: u64, title: &str, content: Option<&str>) -> TabState {
    TabState {
        tab_id,
        title: title.to_string(),
        file_path: Some(std::path::PathBuf::from(format!("/tmp/{title}"))),
        content: content.map(TabContent::from),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
    }
}

/// Build a single-window snapshot around the given tabs.
fn state_with(window_id: i64, tabs: Vec<TabState>) -> WindowsState {
    WindowsState {
        windows: vec![WindowState {
            window_id,
            tabs,
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    }
}

fn memory_db() -> StateDb {
    StateDb::open_in_memory().expect("open in-memory state database")
}

#[test]
fn snapshot_roundtrips_through_the_database() {
    let mut db = memory_db();
    let state = state_with(
        7,
        vec![tab(0, "main.rs", None), tab(3, "notes.md", Some("# draft"))],
    );
    db.apply(&state).expect("apply snapshot");

    let loaded = db.load().expect("load snapshot");
    assert_eq!(loaded.windows.len(), 1);
    let window = &loaded.windows[0];
    assert_eq!(window.window_id, 7);
    assert_eq!(window.tabs.len(), 2);
    assert_eq!(window.tabs[0].tab_id, 0);
    assert_eq!(window.tabs[0].title, "main.rs");
    assert!(window.tabs[0].content.is_none());
    assert_eq!(window.tabs[1].tab_id, 3);
    assert_eq!(
        window.tabs[1].content.as_ref().unwrap(),
        &TabContent::from("# draft")
    );
    assert_eq!(window.active_tab_index, Some(0));
}

#[test]
fn window_bounds_roundtrip() {
    let mut db = memory_db();
    let mut snapshot = state_with(1, vec![tab(0, "a.txt", None)]);
    snapshot.windows[0].window_bounds = SerializedWindowBounds {
        state: "Maximized".to_string(),
        x: 12.0,
        y: 34.0,
        width: 1920.0,
        height: 1080.0,
        display_id: Some(2),
    };
    db.apply(&snapshot).expect("apply snapshot");

    let bounds = db.load().expect("load").windows.remove(0).window_bounds;
    assert_eq!(bounds.state, "Maximized");
    assert!((bounds.x - 12.0).abs() < f32::EPSILON);
    assert!((bounds.height - 1080.0).abs() < f32::EPSILON);
    assert_eq!(bounds.display_id, Some(2));
}

#[test]
fn reapplying_an_identical_snapshot_writes_nothing() {
    let mut db = memory_db();
    let state = state_with(1, vec![tab(0, "a.txt", Some("dirty"))]);
    let first = db.apply(&state).expect("first apply");
    assert_eq!(first.tabs_inserted, 1);

    let second = db.apply(&state).expect("second apply");
    assert!(
        second.is_empty(),
        "an unchanged snapshot must not write any row, got {second:?}"
    );
}

#[test]
fn reordering_tabs_does_not_rewrite_their_content() {
    let mut db = memory_db();
    let first = tab(0, "first.txt", Some("first content"));
    let second = tab(1, "second.txt", Some("second content"));
    db.apply(&state_with(1, vec![first.clone(), second.clone()]))
        .expect("initial apply");

    // Swap the two tabs, which is the operation section 2 of the evaluation
    // measured at ~130 MB of I/O under the JSON snapshot model.
    let stats = db
        .apply(&state_with(1, vec![second, first]))
        .expect("apply reorder");

    assert_eq!(
        stats.tabs_content_written, 0,
        "a reorder must not rewrite buffer text"
    );
    assert_eq!(
        stats.tabs_metadata_updated, 2,
        "both tabs change position and nothing else"
    );
    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].tabs[0].title, "second.txt");
    assert_eq!(loaded.windows[0].tabs[1].title, "first.txt");
    assert_eq!(
        loaded.windows[0].tabs[0].content.as_ref().unwrap(),
        &TabContent::from("second content")
    );
}

#[test]
fn renaming_a_tab_does_not_rewrite_its_content() {
    let mut db = memory_db();
    db.apply(&state_with(1, vec![tab(0, "before.txt", Some("body"))]))
        .expect("initial apply");

    let mut renamed = tab(0, "after.txt", Some("body"));
    renamed.color_tag = Some("red".to_string());
    let stats = db
        .apply(&state_with(1, vec![renamed]))
        .expect("apply rename");

    assert_eq!(stats.tabs_content_written, 0);
    assert_eq!(stats.tabs_metadata_updated, 1);
}

#[test]
fn editing_a_buffer_rewrites_only_that_tab() {
    let mut db = memory_db();
    let untouched = tab(0, "untouched.txt", Some("stable"));
    db.apply(&state_with(
        1,
        vec![untouched.clone(), tab(1, "edited.txt", Some("before"))],
    ))
    .expect("initial apply");

    let stats = db
        .apply(&state_with(
            1,
            vec![untouched, tab(1, "edited.txt", Some("after"))],
        ))
        .expect("apply edit");

    assert_eq!(
        stats.tabs_content_written, 1,
        "only the edited buffer may be rewritten"
    );
    assert_eq!(stats.tabs_metadata_updated, 0);
    let loaded = db.load().expect("load");
    assert_eq!(
        loaded.windows[0].tabs[1].content.as_ref().unwrap(),
        &TabContent::from("after")
    );
}

#[test]
fn a_rope_backed_buffer_matching_the_stored_text_is_not_rewritten() {
    let mut db = memory_db();
    let text = "line one\nline two\n";
    db.apply(&state_with(1, vec![tab(0, "a.txt", Some(text))]))
        .expect("initial apply");

    // What the running application hands the writer is a rope clone, while what
    // came back from disk was a String; the fingerprint has to see through that.
    let mut rope_tab = tab(0, "a.txt", None);
    rope_tab.content = Some(TabContent::Rope(ropey::Rope::from_str(text)));
    let stats = db
        .apply(&state_with(1, vec![rope_tab]))
        .expect("apply rope");

    assert!(
        stats.is_empty(),
        "identical text must compare equal across content representations, got {stats:?}"
    );
}

#[test]
fn closing_a_tab_deletes_its_row() {
    let mut db = memory_db();
    db.apply(&state_with(
        1,
        vec![tab(0, "kept.txt", None), tab(1, "closed.txt", Some("x"))],
    ))
    .expect("initial apply");

    let stats = db
        .apply(&state_with(1, vec![tab(0, "kept.txt", None)]))
        .expect("apply close");

    assert_eq!(stats.tabs_deleted, 1);
    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].tabs.len(), 1);
    assert_eq!(loaded.windows[0].tabs[0].title, "kept.txt");
}

#[test]
fn closing_a_window_deletes_it_with_its_tabs() {
    let mut db = memory_db();
    let mut two_windows = state_with(1, vec![tab(0, "a.txt", None)]);
    two_windows.windows.push(WindowState {
        window_id: 2,
        tabs: vec![tab(0, "b.txt", Some("second window"))],
        active_tab_index: Some(0),
        window_bounds: SerializedWindowBounds::default(),
    });
    db.apply(&two_windows).expect("initial apply");

    let stats = db
        .apply(&state_with(1, vec![tab(0, "a.txt", None)]))
        .expect("apply window close");

    assert_eq!(stats.windows_deleted, 1);
    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows.len(), 1);
    assert_eq!(loaded.windows[0].window_id, 1);
}

#[test]
fn windows_owned_by_another_process_are_left_alone() {
    let mut db = memory_db();
    // A window written by someone else, which this handle never claimed.
    db.apply(&state_with(
        99,
        vec![tab(0, "theirs.txt", Some("their work"))],
    ))
    .expect("seed foreign window");
    db.forget_owned_windows();

    let stats = db
        .apply(&state_with(1, vec![tab(0, "mine.txt", None)]))
        .expect("apply own window");

    assert_eq!(
        stats.windows_deleted, 0,
        "a snapshot must not delete windows it never owned"
    );
    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows.len(), 2);
}

#[test]
fn claiming_persisted_windows_makes_them_deletable() {
    let mut db = memory_db();
    db.apply(&state_with(5, vec![tab(0, "restored.txt", None)]))
        .expect("seed window");
    db.forget_owned_windows();
    db.claim_persisted_windows()
        .expect("claim restored windows");

    let stats = db
        .apply(&WindowsState { windows: vec![] })
        .expect("apply empty snapshot");

    assert_eq!(
        stats.windows_deleted, 1,
        "a restored window the user closed must not reappear on the next launch"
    );
    assert!(db.load().expect("load").windows.is_empty());
}

#[test]
fn active_tab_survives_a_reorder() {
    let mut db = memory_db();
    let first = tab(0, "first.txt", None);
    let second = tab(1, "second.txt", None);
    let mut state = state_with(1, vec![first.clone(), second.clone()]);
    state.windows[0].active_tab_index = Some(1);
    db.apply(&state).expect("initial apply");

    // Move the active tab to the front; it must still be the active one.
    let mut reordered = state_with(1, vec![second, first]);
    reordered.windows[0].active_tab_index = Some(0);
    db.apply(&reordered).expect("apply reorder");

    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].active_tab_index, Some(0));
    assert_eq!(loaded.windows[0].tabs[0].title, "second.txt");
}

#[test]
fn a_window_with_no_active_tab_roundtrips() {
    let mut db = memory_db();
    let mut state = state_with(1, vec![tab(0, "a.txt", None)]);
    state.windows[0].active_tab_index = None;
    db.apply(&state).expect("apply");

    assert_eq!(db.load().expect("load").windows[0].active_tab_index, None);
}

#[test]
fn remote_tabs_roundtrip_without_credentials() {
    let mut db = memory_db();
    let mut remote_tab = tab(0, "remote.txt", Some("remote body"));
    remote_tab.file_path = None;
    remote_tab.remote = Some(SerializedRemoteSpec {
        host: "example.com".to_string(),
        port: 2222,
        user: "alice".to_string(),
        path: "/srv/remote.txt".to_string(),
    });
    db.apply(&state_with(1, vec![remote_tab])).expect("apply");

    let loaded = db.load().expect("load");
    let remote = loaded.windows[0].tabs[0]
        .remote
        .as_ref()
        .expect("remote spec must be restored");
    assert_eq!(remote.host, "example.com");
    assert_eq!(remote.port, 2222);
    assert_eq!(remote.user, "alice");
    assert_eq!(remote.path, "/srv/remote.txt");
    assert!(loaded.windows[0].tabs[0].file_path.is_none());
}

#[test]
fn an_untitled_tab_with_no_path_roundtrips() {
    let mut db = memory_db();
    let mut untitled = tab(0, "Untitled", Some("scratch"));
    untitled.file_path = None;
    db.apply(&state_with(1, vec![untitled])).expect("apply");

    let loaded = db.load().expect("load");
    assert!(loaded.windows[0].tabs[0].file_path.is_none());
    assert_eq!(
        loaded.windows[0].tabs[0].content.as_ref().unwrap(),
        &TabContent::from("scratch")
    );
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_valid_utf8_roundtrips() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let mut db = memory_db();
    // serde's `Serialize for Path` fails on this, which used to abort the whole
    // state save and take every window's unsaved content with it.
    let path = std::path::PathBuf::from(OsStr::from_bytes(b"/tmp/caf\xe9.txt"));
    let mut latin1_tab = tab(0, "café.txt", Some("body"));
    latin1_tab.file_path = Some(path.clone());
    db.apply(&state_with(1, vec![latin1_tab]))
        .expect("a non-UTF-8 path must not fail the save");

    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].tabs[0].file_path.as_ref(), Some(&path));
}

#[test]
fn multibyte_content_roundtrips_with_a_byte_accurate_length() {
    let mut db = memory_db();
    let text = "héllo 文档 🚀\tand\ttabs";
    db.apply(&state_with(1, vec![tab(0, "a.txt", Some(text))]))
        .expect("apply");

    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].tabs[0].content.as_ref().unwrap(), text);
    // The stored length must be bytes, not characters: SQLite's length() counts
    // characters on a TEXT column, which would inflate the effective size cap.
    let stored_len: i64 = db
        .conn
        .query_row("SELECT content_len FROM tabs", [], |row| row.get(0))
        .expect("read content_len");
    assert_eq!(usize::try_from(stored_len).unwrap(), text.len());
}

#[test]
fn windows_are_restored_in_persisted_order() {
    let mut db = memory_db();
    let mut state = state_with(500, vec![tab(0, "first-window.txt", None)]);
    state.windows.push(WindowState {
        window_id: 10,
        tabs: vec![tab(0, "second-window.txt", None)],
        active_tab_index: Some(0),
        window_bounds: SerializedWindowBounds::default(),
    });
    db.apply(&state).expect("apply");

    // Restore order follows the persisted position, not the numeric identity,
    // so window bounds cannot be handed to the wrong window on restart.
    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].window_id, 500);
    assert_eq!(loaded.windows[1].window_id, 10);
}

#[test]
fn allocated_window_ids_are_unique_and_increasing() {
    let first = WindowState::allocate_id();
    let second = WindowState::allocate_id();
    assert!(second > first, "identities must be strictly increasing");
}

#[test]
fn a_reopened_database_keeps_its_rows() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("state.db");
    {
        let mut db = StateDb::open(&path).expect("open");
        db.apply(&state_with(1, vec![tab(0, "persisted.txt", Some("body"))]))
            .expect("apply");
        db.checkpoint();
    }
    let reopened = StateDb::open(&path).expect("reopen");
    let loaded = reopened.load().expect("load");
    assert_eq!(loaded.windows[0].tabs[0].title, "persisted.txt");
    assert_eq!(
        loaded.windows[0].tabs[0].content.as_ref().unwrap(),
        &TabContent::from("body")
    );
}

#[test]
fn a_legacy_json_document_is_imported_with_fresh_identity() {
    let dir = tempfile::tempdir().expect("temp dir");
    let legacy_path = dir.path().join("state.json");
    // Exactly the shape Fulgur 0.10 and earlier wrote: no window or tab ids.
    std::fs::write(
        &legacy_path,
        r#"{
            "windows": [
                {
                    "tabs": [
                        {"title": "a.txt", "file_path": null, "content": "first", "last_saved": null},
                        {"title": "b.txt", "file_path": null, "content": "second", "last_saved": null}
                    ],
                    "active_tab_index": 1,
                    "window_bounds": {
                        "state": "Windowed", "x": 1.0, "y": 2.0,
                        "width": 300.0, "height": 400.0, "display_id": null
                    }
                }
            ]
        }"#,
    )
    .expect("write legacy document");

    let mut db = memory_db();
    let imported = super::import_legacy_json(&legacy_path, &mut db).expect("import legacy");
    assert_eq!(imported, 1);

    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows.len(), 1);
    assert!(
        loaded.windows[0].window_id > 0,
        "an imported window must be given an identity"
    );
    assert_eq!(loaded.windows[0].tabs.len(), 2);
    assert_eq!(loaded.windows[0].tabs[0].tab_id, 0);
    assert_eq!(loaded.windows[0].tabs[1].tab_id, 1);
    assert_eq!(loaded.windows[0].active_tab_index, Some(1));
    assert_eq!(
        loaded.windows[0].tabs[1].content.as_ref().unwrap(),
        &TabContent::from("second")
    );
    assert!(
        legacy_path.exists(),
        "the legacy document must be left on disk"
    );
}

#[test]
fn importing_a_corrupted_legacy_document_falls_back_to_its_backup() {
    let dir = tempfile::tempdir().expect("temp dir");
    let legacy_path = dir.path().join("state.json");
    std::fs::write(&legacy_path, b"not valid json").expect("write corrupted document");
    std::fs::write(
        dir.path().join("state.json.bak"),
        r#"{"windows": [{"tabs": [{"title": "recovered.txt", "file_path": null,
            "content": null, "last_saved": null}], "active_tab_index": 0}]}"#,
    )
    .expect("write backup document");

    let mut db = memory_db();
    super::import_legacy_json(&legacy_path, &mut db).expect("import from backup");

    let loaded = db.load().expect("load");
    assert_eq!(loaded.windows[0].tabs[0].title, "recovered.txt");
}

#[test]
fn saving_a_snapshot_that_was_just_loaded_writes_nothing() {
    let mut db = memory_db();
    db.apply(&state_with(
        42,
        vec![
            tab(0, "a.txt", Some("first buffer")),
            tab(1, "b.txt", Some("second buffer")),
        ],
    ))
    .expect("initial apply");

    // This is the shape of the first save after a session is restored: identity
    // survives the read, so the snapshot the application rebuilds matches what is
    // already stored and no buffer is rewritten.
    let restored = db.load().expect("load");
    let stats = db.apply(&restored).expect("apply restored snapshot");

    assert!(
        stats.is_empty(),
        "re-saving a restored session must not rewrite anything, got {stats:?}"
    );
}

#[test]
fn an_existing_database_is_backed_up_before_being_migrated() {
    let dir = tempfile::tempdir().expect("temp dir");
    let path = dir.path().join("state.db");
    // A zero-length file is a valid empty database at schema version 0, which is
    // exactly what an upgrade from a future release with a newer schema step
    // would look like on the way in.
    std::fs::write(&path, b"").expect("create empty database");

    let db = StateDb::open(&path).expect("open and migrate");
    drop(db);

    let backup = path.with_file_name("state.db.bak");
    assert!(
        backup.exists(),
        "an existing database must be copied aside before its schema is migrated"
    );
    // The backup has to be a usable database, not a partial file copy.
    let restored = StateDb::open(&backup).expect("open the backup");
    assert!(restored.load().expect("load the backup").windows.is_empty());
}

#[test]
fn an_ephemeral_database_reports_itself_as_such() {
    assert!(memory_db().is_ephemeral());
    let dir = tempfile::tempdir().expect("temp dir");
    let db = StateDb::open(&dir.path().join("state.db")).expect("open file database");
    assert!(!db.is_ephemeral());
}
