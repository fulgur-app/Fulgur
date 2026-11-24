use super::{assert_recovery_content, restore_and_snapshot, sample_share_origin, startup_snapshot};
use crate::fulgur::Fulgur;
use crate::fulgur::state::SnapshotKind;
use crate::fulgur::{
    shared_state::SharedAppState,
    state::persistence::{
        SerializedRemoteSpec, SerializedWindowBounds, TabContent, TabState, WindowState,
        WindowsState,
    },
    ui::components_utils::UNTITLED,
};
use crate::test_support::{notification_count, setup_fulgur_with_root as setup_fulgur};
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::{fs, path::Path, sync::Arc};
use tempfile::TempDir;

#[gpui_kit::test]
fn restore_consumes_the_window_slot_and_keeps_other_slots(cx: &mut TestAppContext) {
    let untouched_window = WindowState {
        window_id: 43,
        tabs: vec![TabState {
            tab_id: 20,
            title: UNTITLED.to_string(),
            file_path: None,
            content: Some(TabContent::from("payload of a window not yet opened")),
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
    };
    let mut state = startup_snapshot(TabState {
        tab_id: 10,
        title: UNTITLED.to_string(),
        file_path: None,
        content: Some(TabContent::from("restored payload")),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    });
    state.windows.push(untouched_window);
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let restored = restore_and_snapshot(&fulgur, &mut visual_cx, state);
    assert_recovery_content(&restored.tabs[0], "restored payload");

    let slots = visual_cx.update(|_, cx| {
        let restore_state = Arc::clone(&cx.global::<SharedAppState>().restore_state);
        let slots = restore_state.lock();
        (
            slots.len(),
            slots[0].is_none(),
            slots[1].as_ref().map(|window| window.window_id),
        )
    });
    assert_eq!(
        slots,
        (2, true, Some(43)),
        "the restored slot must be released while the other window keeps its payload and index"
    );

    let restored_again = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.load_state(window, cx, 0);
            this.build_window_state_without_bounds(cx, SnapshotKind::Blocking)
        })
    });
    assert_eq!(
        restored_again
            .tabs
            .iter()
            .map(|tab| tab.tab_id)
            .collect::<Vec<_>>(),
        vec![10],
        "a consumed slot must leave the window as it is instead of restoring again"
    );
}

#[gpui_kit::test]
fn restored_local_recovery_survives_repeated_snapshots(cx: &mut TestAppContext) {
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

    let first = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));
    assert_recovery_content(&first.tabs[0], "recovered local edits");

    let second = restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        WindowsState {
            windows: vec![first],
        },
    );
    assert_recovery_content(&second.tabs[0], "recovered local edits");
    assert_eq!(fs::read_to_string(path).unwrap(), "saved on disk");
}

#[gpui_kit::test]
fn restored_local_recovery_keeps_encoding_and_lossy_flag(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("legacy.txt");
    fs::write(&path, [0x63, 0x61, 0x66, 0xE9]).expect("write legacy file");
    let restored = TabState {
        tab_id: 5,
        title: "legacy.txt".to_string(),
        file_path: Some(path),
        content: Some(TabContent::from("caf\u{e9} edited")),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: Some("windows-1252".to_string()),
        lossy_decode: true,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let snapshot = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));

    assert_eq!(snapshot.tabs[0].encoding.as_deref(), Some("windows-1252"));
    assert!(
        snapshot.tabs[0].lossy_decode,
        "a restored lossy buffer must still require save confirmation"
    );
}

#[gpui_kit::test]
fn restored_remote_tab_keeps_encoding_and_lossy_flag(cx: &mut TestAppContext) {
    let restored = TabState {
        tab_id: 6,
        title: "remote.txt".to_string(),
        file_path: None,
        content: Some(TabContent::from("recovered remote edits")),
        last_saved: None,
        remote: Some(SerializedRemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            path: "/srv/remote.txt".to_string(),
        }),
        log_view: false,
        color_tag: None,
        share: None,
        encoding: Some("Shift_JIS".to_string()),
        lossy_decode: true,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let snapshot = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));

    assert_eq!(snapshot.tabs[0].encoding.as_deref(), Some("Shift_JIS"));
    assert!(snapshot.tabs[0].lossy_decode);
}

#[gpui_kit::test]
fn only_a_restored_remote_tab_without_edits_awaits_its_content(cx: &mut TestAppContext) {
    let remote_tab = |tab_id, content: Option<&str>| TabState {
        tab_id,
        title: "remote.txt".to_string(),
        file_path: None,
        content: content.map(TabContent::from),
        last_saved: None,
        remote: Some(SerializedRemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            path: "/srv/remote.txt".to_string(),
        }),
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let mut state = startup_snapshot(remote_tab(7, None));
    state.windows[0].tabs.push(remote_tab(8, Some("recovered")));

    restore_and_snapshot(&fulgur, &mut visual_cx, state);

    visual_cx.update(|_window, cx| {
        let tabs = &fulgur.read(cx).tabs;
        let (placeholder, recovered) = (&tabs[0], &tabs[1]);
        let placeholder = placeholder.read(cx).as_editor().expect("editor tab");
        let recovered = recovered.read(cx).as_editor().expect("editor tab");
        assert!(placeholder.awaiting_content);
        assert!(!placeholder.content.read(cx).is_editable());
        assert!(!recovered.awaiting_content);
        assert!(recovered.content.read(cx).is_editable());
    });
}

#[gpui_kit::test]
fn restored_remote_recovery_survives_repeated_snapshots(cx: &mut TestAppContext) {
    let restored = TabState {
        tab_id: 8,
        title: "remote.txt".to_string(),
        file_path: None,
        content: Some(TabContent::from("recovered remote edits")),
        last_saved: None,
        remote: Some(SerializedRemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: "alice".to_string(),
            path: "/srv/remote.txt".to_string(),
        }),
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let first = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));
    assert_recovery_content(&first.tabs[0], "recovered remote edits");

    let second = restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        WindowsState {
            windows: vec![first],
        },
    );
    assert_recovery_content(&second.tabs[0], "recovered remote edits");
}

#[gpui_kit::test]
fn restored_shared_tab_keeps_its_origin(cx: &mut TestAppContext) {
    let origin = sample_share_origin();
    let restored = TabState {
        tab_id: 9,
        title: "received.md".to_string(),
        file_path: None,
        content: Some(TabContent::from("# received")),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: Some(origin.clone()),
        encoding: None,
        lossy_decode: false,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let snapshot = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));

    assert_recovery_content(&snapshot.tabs[0], "# received");
    assert_eq!(snapshot.tabs[0].share.as_ref(), Some(&origin));
}

#[gpui_kit::test]
fn restored_untitled_tab_installs_color_provider(cx: &mut TestAppContext) {
    let restored = TabState {
        tab_id: 11,
        title: UNTITLED.to_string(),
        file_path: None,
        content: Some(TabContent::from("#B2C299")),
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(restored));

    let (highlight_colors_setting, has_color_provider) =
        fulgur.read_with(&visual_cx, |this, cx| {
            let editor_tab = this
                .tabs
                .first()
                .and_then(|tab| tab.read(cx).as_editor())
                .expect("expected a restored editor tab");
            (
                this.settings.editor_settings.highlight_colors,
                editor_tab
                    .content
                    .read(cx)
                    .lsp()
                    .document_color_provider
                    .is_some(),
            )
        });
    assert!(highlight_colors_setting);
    assert!(
        has_color_provider,
        "a restored unsaved buffer must get the same color provider as a new tab"
    );
}

/// Build the persisted state of a clean local tab, which stores no buffer.
///
/// ### Arguments
/// - `tab_id`: The persisted tab id
/// - `path`: The local file the tab holds
///
/// ### Returns
/// - `TabState`: A clean tab restored by reading `path`
fn clean_local_tab(tab_id: u64, path: &Path) -> TabState {
    TabState {
        tab_id,
        title: path
            .file_name()
            .map_or_default(|name| name.to_string_lossy().into_owned()),
        file_path: Some(path.to_path_buf()),
        content: None,
        last_saved: None,
        remote: None,
        log_view: false,
        color_tag: None,
        share: None,
        encoding: None,
        lossy_decode: false,
    }
}

/// Restore a startup snapshot, let the background file reads land, then snapshot.
///
/// ### Arguments
/// - `fulgur`: The application entity whose tabs should be restored
/// - `visual_cx`: The visual test context containing the application window
/// - `state`: The startup snapshot to restore
///
/// ### Returns
/// - `WindowState`: The window snapshot taken once every placeholder resolved
fn restore_and_settle(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    state: WindowsState,
) -> WindowState {
    restore_and_snapshot(fulgur, visual_cx, state);
    visual_cx.run_until_parked();
    fulgur.read_with(visual_cx, |this, cx| {
        this.build_window_state_without_bounds(cx, SnapshotKind::Blocking)
    })
}

#[gpui_kit::test]
fn clean_local_tab_restores_as_a_placeholder_until_its_read_lands(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("notes.txt");
    fs::write(&path, "restored from disk\n").expect("write file");
    let disk_modified = fs::metadata(&path)
        .and_then(|metadata| metadata.modified())
        .expect("read mtime");
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        startup_snapshot(clean_local_tab(7, &path)),
    );

    visual_cx.update(|_window, cx| {
        let tab = fulgur.read(cx).tabs[0].read(cx);
        let editor = tab.as_editor().expect("editor tab");
        assert!(
            editor.awaiting_content,
            "the window must not wait for the file before its first frame"
        );
        assert!(!editor.content.read(cx).is_editable());
        assert_eq!(editor.content.read(cx).text().len(), 0);
    });

    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        let tab = fulgur.read(cx).tabs[0].read(cx);
        let editor = tab.as_editor().expect("editor tab");
        assert!(!editor.awaiting_content);
        assert!(!editor.modified);
        assert!(editor.content.read(cx).is_editable());
        assert_eq!(
            editor.content.read(cx).text().to_string(),
            "restored from disk\n"
        );
        assert_eq!(editor.file_last_modified, Some(disk_modified));
    });
}

#[gpui_kit::test]
fn restored_csv_placeholder_detects_its_delimiter_once_loaded(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("table.csv");
    fs::write(&path, "a;b;c\n1;2;3\n4;5;6\n").expect("write csv");
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    restore_and_settle(
        &fulgur,
        &mut visual_cx,
        startup_snapshot(clean_local_tab(7, &path)),
    );

    visual_cx.update(|_window, cx| {
        let tab = fulgur.read(cx).tabs[0].read(cx);
        let editor = tab.as_editor().expect("editor tab");
        assert_eq!(editor.csv_delimiter, b';');
    });
}

#[gpui_kit::test]
fn save_of_a_restored_local_placeholder_is_refused(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("keep.txt");
    fs::write(&path, "original").expect("write file");
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    restore_and_snapshot(
        &fulgur,
        &mut visual_cx,
        startup_snapshot(clean_local_tab(7, &path)),
    );

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.save_file(window, cx));
    });
    visual_cx.run_until_parked();

    assert_eq!(
        fs::read_to_string(&path).expect("read file"),
        "original",
        "saving the empty placeholder would have truncated the file"
    );
    assert_eq!(notification_count(&mut visual_cx), 1);
}

#[gpui_kit::test]
fn restore_closes_clean_tab_whose_file_now_looks_binary(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("became_binary.txt");
    fs::write(&path, [0x7F, b'E', b'L', b'F', 0x00, 0x01]).expect("write binary file");
    let text_path = temp_dir.path().join("text.txt");
    fs::write(&text_path, "text").expect("write text file");
    let mut state = startup_snapshot(clean_local_tab(7, &path));
    state.windows[0].tabs.push(clean_local_tab(8, &text_path));
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let restored = restore_and_settle(&fulgur, &mut visual_cx, state);

    let restored_paths: Vec<_> = restored
        .tabs
        .iter()
        .filter_map(|tab| tab.file_path.clone())
        .collect();
    assert_eq!(
        restored_paths,
        vec![text_path],
        "a clean tab whose file now looks binary must be closed, as opening it would be refused"
    );
    assert_eq!(notification_count(&mut visual_cx), 1);
}

/// On Unix a directory opens as a file but fails on read, which reproduces a
/// read error after a successful open.
#[cfg(unix)]
#[gpui_kit::test]
fn restore_replaces_the_only_tab_whose_file_fails_to_read(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("unreadable.txt");
    fs::create_dir(&path).expect("create directory standing in for the file");
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let restored = restore_and_settle(
        &fulgur,
        &mut visual_cx,
        startup_snapshot(clean_local_tab(7, &path)),
    );

    assert!(
        restored
            .tabs
            .iter()
            .all(|tab| tab.file_path.as_deref() != Some(path.as_path())),
        "a clean tab whose file fails to read must be closed, as no buffer was persisted"
    );
    let live_tab_count = fulgur.read_with(&visual_cx, |this, _| this.tabs.len());
    assert_eq!(
        live_tab_count, 1,
        "a window left without tabs gets an untitled one"
    );
    assert_eq!(notification_count(&mut visual_cx), 1);
}
