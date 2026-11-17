use super::*;
use crate::fulgur::state::persistence::{
    DeviceKeyPins, SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState,
};
use std::path::Path;
use std::sync::Arc;
use std::thread;
use tempfile::tempdir;

fn sample_state(label: &str) -> WindowsState {
    state_with_window_id(label, 1)
}

fn state_with_window_id(label: &str, window_id: i64) -> WindowsState {
    WindowsState {
        windows: vec![WindowState {
            window_id,
            tabs: vec![TabState {
                tab_id: 0,
                title: label.to_string(),
                file_path: None,
                content: Some(TabContent::from(label)),
                last_saved: None,
                remote: None,
                log_view: false,
                color_tag: None,
                share: None,
                encoding: None,
                lossy_decode: false,
            }],
            active_tab_index: Some(0),
            window_bounds: SerializedWindowBounds::default(),
        }],
    }
}

/// Open a writer over a fresh database at `path`.
fn writer_at(path: &Path, throttle: Duration) -> StateWriter {
    let db = StateDb::open(path).expect("open state database");
    StateWriter::with_throttle(Some(db), throttle)
}

/// Read back the title of the single tab persisted at `path`.
fn persisted_title(path: &Path) -> String {
    WindowsState::load_from_path(path)
        .expect("load persisted state")
        .windows[0]
        .tabs[0]
        .title
        .clone()
}

/// Read back every persisted window.
fn persisted_windows(path: &Path) -> Vec<WindowState> {
    WindowsState::load_from_path(path)
        .expect("load persisted state")
        .windows
}

#[test]
fn writer_persists_snapshot() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, SAVE_THROTTLE);
    writer.save_blocking(sample_state("solo")).unwrap();
    assert_eq!(persisted_title(&path), "solo");
}

#[test]
fn writer_async_save_persists_snapshot_on_shutdown() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    writer.save_async(sample_state("async"));
    // Dropping the writer flushes whatever the throttle is still holding.
    drop(writer);
    assert_eq!(persisted_title(&path), "async");
}

#[test]
fn writer_flattens_rope_backed_content() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, SAVE_THROTTLE);
    let mut state = sample_state("rope");
    let text = "ünsaved\ttext with \"quotes\"\nand a newline";
    state.windows[0].tabs[0].content = Some(TabContent::Rope(ropey::Rope::from_str(text)));
    writer.save_blocking(state).unwrap();
    let windows = persisted_windows(&path);
    assert_eq!(windows[0].tabs[0].content.as_ref().unwrap(), text);
}

/// Throttle long enough that no held-back save can escape on its own timer,
/// so the coalescing tests below stay deterministic on slow machines.
const NEVER_ELAPSES: Duration = Duration::from_mins(1);

#[test]
fn writer_coalesces_a_burst_of_async_saves() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    // Seed the throttle so no snapshot of the burst can escape on the
    // leading edge, which makes the outcome deterministic.
    writer.save_blocking(sample_state("seed")).unwrap();
    for i in 0..10 {
        writer.save_async(sample_state(&format!("burst-{i}")));
    }
    drop(writer);
    assert_eq!(
        persisted_title(&path),
        "burst-9",
        "the burst must collapse into the newest snapshot"
    );
}

#[test]
fn writer_holds_at_most_one_pending_snapshot() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    writer.save_blocking(sample_state("seed")).unwrap();
    let burst = CHANNEL_CAPACITY * 4;
    for i in 0..burst {
        writer.save_async(sample_state(&format!("burst-{i}")));
    }
    let held = writer.mailbox.take().expect("a snapshot must be waiting");
    assert_eq!(
        held.windows[0].tabs[0].title,
        format!("burst-{}", burst - 1),
        "the surviving snapshot must be the newest one"
    );
    assert!(
        !writer.mailbox.has_pending(),
        "a burst must not queue one snapshot per request"
    );
}

#[test]
fn writer_throttled_async_save_lands_once_the_period_elapses() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, Duration::from_millis(500));
    writer.save_blocking(sample_state("first")).unwrap();
    writer.save_async(sample_state("second"));
    assert_eq!(
        persisted_title(&path),
        "first",
        "the async save must be held back by the throttle"
    );
    thread::sleep(Duration::from_millis(900));
    assert_eq!(persisted_title(&path), "second");
}

#[test]
fn writer_blocking_save_supersedes_a_held_back_async_save() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    writer.save_blocking(sample_state("first")).unwrap();
    writer.save_async(sample_state("superseded"));
    writer.save_blocking(sample_state("last")).unwrap();
    assert!(
        !writer.mailbox.has_pending(),
        "the superseded snapshot must be dropped, not written later"
    );
    drop(writer);
    assert_eq!(persisted_title(&path), "last");
}

#[test]
fn writer_flushes_a_held_back_save_on_shutdown() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    writer.save_blocking(sample_state("first")).unwrap();
    writer.save_async(sample_state("held-back"));
    drop(writer);
    assert_eq!(persisted_title(&path), "held-back");
}

/// Read back the remote history persisted at `path`.
fn persisted_remote_history(path: &Path) -> RemoteHistory {
    StateDb::open(path)
        .expect("reopen state database")
        .load_remote_history()
        .expect("load remote history")
}

/// Build a remote history from URLs ordered most recent first.
fn remote_history(urls: &[&str]) -> RemoteHistory {
    RemoteHistory::from_urls(urls.iter().map(ToString::to_string))
}

#[test]
fn writer_persists_the_remote_history_without_waiting_for_the_throttle() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    writer.save_blocking(sample_state("seed")).unwrap();
    let history = remote_history(&["ssh://host/"]);
    writer.save_remote_history(history.clone());
    // A blocking save is processed after the history wakeup, so once it
    // returns the history has been written even though the throttle never
    // elapses.
    writer.save_blocking(sample_state("seed")).unwrap();
    assert_eq!(persisted_remote_history(&path), history);
}

#[test]
fn writer_keeps_only_the_newest_pending_remote_history() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    writer
        .mailbox
        .put_remote_history(remote_history(&["ssh://old/"]));
    writer
        .mailbox
        .put_remote_history(remote_history(&["ssh://new/"]));
    assert_eq!(
        writer.mailbox.take_remote_history(),
        Some(remote_history(&["ssh://new/"]))
    );
    assert_eq!(writer.mailbox.take_remote_history(), None);
}

#[test]
fn writer_flushes_a_pending_remote_history_on_shutdown() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    let history = remote_history(&["ssh://b/", "ssh://a/"]);
    writer.save_remote_history(history.clone());
    drop(writer);
    assert_eq!(persisted_remote_history(&path), history);
}

#[test]
fn writer_blocking_save_does_not_drop_a_pending_remote_history() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    let history = remote_history(&["ssh://host/etc/hosts"]);
    writer.mailbox.put_remote_history(history.clone());
    writer.save_blocking(sample_state("blocking")).unwrap();
    drop(writer);
    assert_eq!(persisted_title(&path), "blocking");
    assert_eq!(persisted_remote_history(&path), history);
}

#[test]
fn writer_persists_device_key_pins_alongside_the_remote_history() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = writer_at(&path, NEVER_ELAPSES);
    let history = remote_history(&["ssh://host/"]);
    let pins = DeviceKeyPins::from_entries([(
        "home".to_string(),
        "laptop".to_string(),
        "age1laptop".to_string(),
    )]);
    writer.save_remote_history(history.clone());
    writer.save_device_key_pins(pins.clone());
    drop(writer);
    let db = StateDb::open(&path).expect("reopen state database");
    assert_eq!(db.load_device_key_pins().expect("load pins"), pins);
    assert_eq!(db.load_remote_history().expect("load history"), history);
}

#[test]
fn writer_reports_an_error_when_no_database_is_available() {
    let writer = StateWriter::with_throttle(None, SAVE_THROTTLE);
    assert_eq!(
        writer.persistence_error(),
        Some("no state database is available")
    );
    assert!(writer.save_blocking(sample_state("nowhere")).is_err());
}

#[test]
fn writer_reports_ephemeral_fallback_as_non_durable() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    std::fs::write(&path, b"not a database").unwrap();
    let db = StateDb::open_or_fallback(&path).expect("fallback database");
    let writer = StateWriter::with_throttle(Some(db), SAVE_THROTTLE);

    assert!(
        writer
            .persistence_error()
            .is_some_and(|error| error.contains(&path.display().to_string()))
    );
    let error = writer
        .save_blocking(sample_state("temporary"))
        .expect_err("in-memory fallback must not report a durable save");
    assert!(error.to_string().contains("updated only in memory"));
}

#[test]
fn writer_serializes_concurrent_save_requests() {
    let dir = tempdir().unwrap();
    let path = dir.path().join("state.db");
    let writer = Arc::new(writer_at(&path, SAVE_THROTTLE));
    let mut handles = Vec::new();
    for i in 0..16 {
        let writer = Arc::clone(&writer);
        let state = state_with_window_id(&format!("thread-{i}"), i64::from(i) + 1);
        handles.push(thread::spawn(move || writer.save_blocking(state)));
    }
    for handle in handles {
        assert!(handle.join().unwrap().is_ok());
    }
    drop(writer);
    // Each snapshot describes the whole session and owns a single window, so
    // the last writer's window is the one left standing. What matters is that
    // the interleaving produced exactly one coherent window, not a mix.
    let windows = persisted_windows(&path);
    assert_eq!(windows.len(), 1);
    assert_eq!(windows[0].tabs.len(), 1);
    assert!(windows[0].tabs[0].title.starts_with("thread-"));
}
