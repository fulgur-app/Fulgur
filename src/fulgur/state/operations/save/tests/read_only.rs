use super::{assert_recovery_content, restore_and_snapshot, startup_snapshot};
use crate::fulgur::{
    Fulgur,
    shared_state::SharedAppState,
    state::persistence::{TabContent, TabState, WindowsState},
};
use crate::test_support::setup_fulgur_with_root as setup_fulgur;
use gpui_kit::{BorrowAppContext, Entity, TestAppContext, VisualTestContext};
use std::fs;
use tempfile::TempDir;

/// Set whether the process behaves as a `--read-only` session.
///
/// ### Arguments
/// - `cx`: The visual test context
/// - `read_only`: The value of the session flag
fn set_read_only_session(cx: &mut VisualTestContext, read_only: bool) {
    cx.update(|_, cx| {
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.read_only_session = read_only;
        });
    });
}

/// Read the lock state of the first tab of a window.
///
/// ### Arguments
/// - `fulgur`: The application entity holding the tab
/// - `cx`: The visual test context
///
/// ### Returns
/// - `(bool, bool)`: The tab's `read_only` flag and whether its buffer accepts user edits
fn first_tab_lock_state(fulgur: &Entity<Fulgur>, cx: &mut VisualTestContext) -> (bool, bool) {
    fulgur.read_with(cx, |this, cx| {
        let editor = this.tabs[0].read(cx).as_editor().expect("editor tab");
        (editor.read_only, editor.content.read(cx).is_editable())
    })
}

/// Build the persisted state of an untitled tab.
///
/// ### Arguments
/// - `content`: The persisted buffer text
///
/// ### Returns
/// - `TabState`: The untitled tab state
fn untitled_tab_state(content: &str) -> TabState {
    TabState {
        tab_id: 7,
        title: "Untitled".to_string(),
        file_path: None,
        content: Some(TabContent::from(content)),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    }
}

#[gpui_kit::test]
fn read_only_session_locks_restored_tabs_without_persisting_the_lock(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("notes.txt");
    fs::write(&path, "saved on disk").expect("write saved file");
    let restored = TabState {
        tab_id: 7,
        title: "notes.txt".to_string(),
        file_path: Some(path.clone()),
        content: Some(TabContent::from("recovered local edits")),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    set_read_only_session(&mut visual_cx, true);
    let snapshot = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));
    assert_eq!(
        first_tab_lock_state(&fulgur, &mut visual_cx),
        (true, false),
        "a --read-only launch must lock restored tabs"
    );
    assert_recovery_content(&snapshot.tabs[0], "recovered local edits");

    set_read_only_session(&mut visual_cx, false);
    let reopened = restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        WindowsState {
            windows: vec![snapshot],
        },
    );
    assert_eq!(
        first_tab_lock_state(&fulgur, &mut visual_cx),
        (false, true),
        "the next launch without the flag must find the tab editable"
    );
    assert_recovery_content(&reopened.tabs[0], "recovered local edits");
}

#[gpui_kit::test]
fn read_only_session_locks_restored_untitled_tabs_with_content(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    set_read_only_session(&mut visual_cx, true);

    restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        startup_snapshot(untitled_tab_state("unsaved notes")),
    );

    assert_eq!(first_tab_lock_state(&fulgur, &mut visual_cx), (true, false));
}

#[gpui_kit::test]
fn read_only_session_keeps_restored_blank_tabs_editable(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    set_read_only_session(&mut visual_cx, true);

    restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        startup_snapshot(untitled_tab_state("")),
    );

    assert_eq!(first_tab_lock_state(&fulgur, &mut visual_cx), (false, true));
}
