use super::{
    assert_recovery_content, configured_tab_states, restore_and_snapshot, sample_remote_location,
    sample_share_origin, startup_snapshot, tab_states_with,
};
use crate::fulgur::{
    Fulgur,
    editor_tab::{TabLocation, replace_editor_text},
    files::file_watcher::{FileWatchEvent, test_helpers::settle_file_checks},
    state::persistence::TabState,
    tab::Tab,
    ui::{components_utils::UNTITLED, tabs::color_tag::ColorTag},
};
use crate::test_support::setup_fulgur_with_root as setup_fulgur;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::fs;
use tempfile::TempDir;

#[gpui_kit::test]
fn dirty_file_tab_persists_content_when_setting_is_enabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local("/tmp/notes.txt".into()),
        true,
    );
    assert_eq!(tabs.len(), 1);
    assert!(
        tabs[0].content.is_some(),
        "unsaved content must be persisted when the setting is enabled"
    );
}

/// Give the first tab a saved local file baseline, then replace its text.
///
/// ### Arguments
/// - `fulgur`: The application entity whose first tab is configured
/// - `cx`: The visual test context containing the application window
/// - `emit_change`: Whether the replacement emits a change event, as user edits do
///
/// ### Returns
/// - `Vec<TabState>`: The persisted tabs of the window snapshot
fn same_length_edit_tab_states(
    fulgur: &Entity<Fulgur>,
    cx: &mut VisualTestContext,
    emit_change: bool,
) -> Vec<TabState> {
    let temp_dir = TempDir::new().expect("create temp dir");
    let path = temp_dir.path().join("notes.txt");
    fs::write(&path, "saved text").expect("write saved file");
    cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.settings.app_settings.persist_unsaved_buffers = true;
            this.update_active_editor_tab(cx, |editor_tab, _| {
                editor_tab.location = TabLocation::Local(path.clone());
                editor_tab.set_original_content_from_str("saved text");
                editor_tab.modified = false;
            });
            let content = this
                .active_tab(cx)
                .and_then(Tab::as_editor)
                .expect("expected an editor tab")
                .content
                .clone();
            content.update(cx, |content, cx| {
                if emit_change {
                    replace_editor_text(content, "SAVED TEXT", window, cx);
                } else {
                    content.set_value("SAVED TEXT", window, cx);
                }
            });
        });
    });
    cx.run_until_parked();
    fulgur.read_with(cx, |this, cx| {
        this.build_window_state_without_bounds(cx).tabs
    })
}

#[gpui_kit::test]
fn same_length_edit_is_persisted_through_the_modified_flag(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = same_length_edit_tab_states(&fulgur, &mut visual_cx, true);
    assert_recovery_content(&tabs[0], "SAVED TEXT");
}

#[gpui_kit::test]
fn snapshot_does_not_fingerprint_a_clean_tab_of_matching_length(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = same_length_edit_tab_states(&fulgur, &mut visual_cx, false);
    assert!(
        tabs[0].content.is_none(),
        "only a fingerprint could spot this silent replacement, and snapshots must not hash"
    );
}

#[gpui_kit::test]
fn dirty_file_tab_persists_path_only_when_setting_is_disabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local("/tmp/notes.txt".into()),
        false,
    );
    assert_eq!(tabs.len(), 1, "the tab itself must still be restored");
    assert!(
        tabs[0].content.is_none(),
        "unsaved content must not be persisted when the setting is disabled"
    );
    assert!(tabs[0].file_path.is_some());
}

#[gpui_kit::test]
fn externally_deleted_file_buffer_survives_a_restart(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("deleted.txt");
    let text = "only remaining copy";
    configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local(path.clone()),
        true,
        text,
        |editor_tab| {
            editor_tab.title = "deleted.txt".into();
            editor_tab.set_original_content_from_str(text);
            editor_tab.modified = false;
        },
    );
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.handle_file_watch_event(FileWatchEvent::Deleted(path), window, cx);
        });
    });
    settle_file_checks(&mut visual_cx);

    let snapshot = fulgur.read_with(
        &visual_cx,
        crate::fulgur::Fulgur::build_window_state_without_bounds,
    );
    let persisted = snapshot.tabs.into_iter().next().expect("expected a tab");
    assert_recovery_content(&persisted, text);

    let restored = restore_and_snapshot(&fulgur, &mut visual_cx, startup_snapshot(persisted)).tabs;
    assert_eq!(restored.len(), 1, "the restart must bring the tab back");
    assert_recovery_content(&restored[0], text);
    assert_eq!(restored[0].title, "deleted.txt");
    assert!(
        restored[0].file_path.is_none(),
        "the restored buffer no longer has a file behind it"
    );
}

#[gpui_kit::test]
fn clean_file_tab_persists_content_only_when_its_file_is_missing(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let dir = TempDir::new().expect("failed to create temp dir");
    let path = dir.path().join("watching_disabled.txt");
    let text = "unedited text";
    fs::write(&path, text).expect("failed to write test file");
    let snapshot_clean_tab = |visual_cx: &mut VisualTestContext| {
        configured_tab_states(
            &fulgur,
            visual_cx,
            TabLocation::Local(path.clone()),
            true,
            text,
            |editor_tab| {
                editor_tab.set_original_content_from_str(text);
                editor_tab.modified = false;
            },
        )
    };

    let while_present = snapshot_clean_tab(&mut visual_cx);
    assert!(
        while_present[0].content.is_none(),
        "a clean tab whose file exists is reloaded from that file"
    );

    fs::remove_file(&path).expect("failed to delete test file");
    let once_missing = snapshot_clean_tab(&mut visual_cx);
    assert_recovery_content(&once_missing[0], text);
}

#[gpui_kit::test]
fn untitled_tab_is_dropped_when_setting_is_disabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, false);
    assert!(
        tabs.is_empty(),
        "an untitled tab carries nothing but its unsaved content, so it must be dropped"
    );
}

#[gpui_kit::test]
fn untitled_tab_is_persisted_when_setting_is_enabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, true);
    assert_eq!(tabs.len(), 1);
    assert!(tabs[0].content.is_some());
}

#[gpui_kit::test]
fn large_dirty_file_tab_persists_path_only(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local("/tmp/huge.log".into()),
        true,
        "dirty content",
        |editor_tab| editor_tab.large_file = true,
    );
    assert_eq!(tabs.len(), 1, "the large file must still be reopened");
    assert!(
        tabs[0].content.is_none(),
        "edits to a large file must not be embedded in the state database"
    );
    assert!(tabs[0].file_path.is_some());
    assert!(tabs[0].last_saved.is_none());
}

#[gpui_kit::test]
fn large_untitled_tab_is_dropped(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Untitled,
        true,
        "dirty content",
        |editor_tab| editor_tab.large_file = true,
    );
    assert!(
        tabs.is_empty(),
        "a large untitled buffer has no path to reopen, so it must be dropped"
    );
}

#[gpui_kit::test]
fn large_shared_tab_is_dropped(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Shared(sample_share_origin()),
        true,
        "dirty content",
        |editor_tab| editor_tab.large_file = true,
    );
    assert!(
        tabs.is_empty(),
        "a large shared buffer has no path to reopen, so it must be dropped"
    );
}

#[gpui_kit::test]
fn empty_untitled_tab_is_skipped(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Untitled,
        true,
        "",
        |editor_tab| assert!(editor_tab.title.starts_with(UNTITLED)),
    );
    assert!(
        tabs.is_empty(),
        "an empty tab that still has its default title carries nothing to restore"
    );
}

#[gpui_kit::test]
fn empty_renamed_untitled_tab_is_persisted(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Untitled,
        true,
        "",
        |editor_tab| editor_tab.title = "Scratch".into(),
    );
    assert_eq!(tabs.len(), 1, "a renamed tab must survive even when empty");
    assert_eq!(tabs[0].title, "Scratch");
}

#[gpui_kit::test]
fn dirty_remote_tab_persists_content_when_setting_is_enabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(&fulgur, &mut visual_cx, sample_remote_location(), true);
    assert_eq!(tabs.len(), 1);
    assert_recovery_content(&tabs[0], "dirty content");
    assert!(tabs[0].file_path.is_none());
    assert_eq!(
        tabs[0].remote.as_ref().map(|remote| remote.path.as_str()),
        Some("/srv/remote.txt")
    );
}

#[gpui_kit::test]
fn dirty_remote_tab_persists_location_only_when_setting_is_disabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(&fulgur, &mut visual_cx, sample_remote_location(), false);
    assert_eq!(
        tabs.len(),
        1,
        "the remote tab itself must still be restored"
    );
    assert!(
        tabs[0].content.is_none(),
        "unsaved remote content must not be persisted when the setting is disabled"
    );
    assert!(tabs[0].remote.is_some());
}

#[gpui_kit::test]
fn snapshot_carries_color_tag_and_log_view(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = configured_tab_states(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local("/tmp/server.log".into()),
        true,
        "dirty content",
        |editor_tab| {
            editor_tab.color_tag = Some(ColorTag::Red);
            editor_tab.log_view = true;
        },
    );
    assert_eq!(tabs.len(), 1);
    assert_eq!(tabs[0].color_tag.as_deref(), Some(ColorTag::Red.key()));
    assert!(tabs[0].log_view);
}

#[gpui_kit::test]
fn shared_tab_persists_its_origin_when_setting_is_enabled(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let origin = sample_share_origin();
    let tabs = tab_states_with(
        &fulgur,
        &mut visual_cx,
        TabLocation::Shared(origin.clone()),
        true,
    );
    assert_eq!(tabs.len(), 1);
    assert!(tabs[0].content.is_some());
    assert!(tabs[0].file_path.is_none());
    assert_eq!(tabs[0].share.as_ref(), Some(&origin));
}

#[gpui_kit::test]
fn saved_shared_tab_no_longer_persists_its_origin(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let tabs = tab_states_with(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local("/tmp/received.txt".into()),
        true,
    );
    assert_eq!(tabs.len(), 1);
    assert!(
        tabs[0].share.is_none(),
        "a shared tab saved to a file must go back to the regular file state"
    );
}
