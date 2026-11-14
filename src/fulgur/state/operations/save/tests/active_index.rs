use super::{restore_and_snapshot, tab_states_with};
use crate::fulgur::{
    editor_tab::TabLocation,
    languages::supported_languages::SupportedLanguage,
    state::persistence::{SerializedWindowBounds, TabContent, TabState, WindowState, WindowsState},
    tab::Tab,
};
use crate::test_support::setup_fulgur_with_root as setup_fulgur;
use gpui_kit::TestAppContext;
use tempfile::TempDir;

#[gpui_kit::test]
fn active_index_ignores_tabs_omitted_from_snapshot(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let snapshot = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.settings.app_settings.persist_unsaved_buffers = true;
            this.open_settings(window, cx);
            this.new_tab(window, cx);

            let active_tab = this
                .active_tab_entity(cx)
                .expect("expected the new editor tab to be active");
            active_tab.update(cx, |tab, cx| {
                tab.as_editor_mut()
                    .expect("expected an editor tab")
                    .content
                    .update(cx, |content, cx| {
                        content.set_value("persisted active tab", window, cx);
                    });
            });

            this.build_window_state_without_bounds(cx)
        })
    });

    assert_eq!(snapshot.tabs.len(), 1);
    assert_eq!(snapshot.active_tab_index, Some(0));
}

#[gpui_kit::test]
fn restore_keeps_active_tab_identity_when_an_earlier_tab_is_skipped(cx: &mut TestAppContext) {
    let temp_dir = TempDir::new().expect("create temp dir");
    let state = WindowsState {
        windows: vec![WindowState {
            window_id: 42,
            tabs: vec![
                TabState {
                    tab_id: 10,
                    title: "missing.txt".to_string(),
                    file_path: Some(temp_dir.path().join("missing.txt")),
                    content: None,
                    last_saved: None,
                    remote: None,
                    log_view: false,
                    color_tag: None,
                    share: None,
                    encoding: None,
                    lossy_decode: false,
                },
                TabState {
                    tab_id: 11,
                    title: "active.txt".to_string(),
                    file_path: None,
                    content: Some(TabContent::from("active content")),
                    last_saved: None,
                    remote: None,
                    log_view: false,
                    color_tag: None,
                    share: None,
                    encoding: None,
                    lossy_decode: false,
                },
                TabState {
                    tab_id: 12,
                    title: "other.txt".to_string(),
                    file_path: None,
                    content: Some(TabContent::from("other content")),
                    last_saved: None,
                    remote: None,
                    log_view: false,
                    color_tag: None,
                    share: None,
                    encoding: None,
                    lossy_decode: false,
                },
            ],
            active_tab_index: Some(1),
            window_bounds: SerializedWindowBounds::default(),
        }],
    };
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let restored = restore_and_snapshot(&fulgur, &mut visual_cx, state);

    assert_eq!(
        restored
            .tabs
            .iter()
            .map(|tab| tab.tab_id)
            .collect::<Vec<_>>(),
        vec![11, 12]
    );
    assert_eq!(restored.active_tab_index, Some(0));
}

#[gpui_kit::test]
fn active_index_maps_preview_tab_to_its_source(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let snapshot = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.settings.app_settings.persist_unsaved_buffers = true;
            this.update_active_editor_tab(cx, |editor_tab, _| {
                editor_tab.language = SupportedLanguage::Markdown;
            });
            let active_tab = this
                .active_tab_entity(cx)
                .expect("expected an active editor tab");
            active_tab.update(cx, |tab, cx| {
                tab.as_editor_mut()
                    .expect("expected an editor tab")
                    .content
                    .update(cx, |content, cx| {
                        content.set_value("# notes", window, cx);
                    });
            });
            this.open_markdown_preview_tab(window, cx);
            assert!(matches!(this.active_tab(cx), Some(Tab::MarkdownPreview(_))));
            this.build_window_state_without_bounds(cx)
        })
    });
    assert_eq!(snapshot.tabs.len(), 1, "preview tabs must not be persisted");
    assert_eq!(snapshot.active_tab_index, Some(0));
}

#[gpui_kit::test]
fn active_index_is_none_when_settings_tab_is_active(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    tab_states_with(&fulgur, &mut visual_cx, TabLocation::Untitled, true);
    let snapshot = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.open_settings(window, cx);
            this.build_window_state_without_bounds(cx)
        })
    });
    assert_eq!(snapshot.tabs.len(), 1);
    assert_eq!(snapshot.active_tab_index, None);
}
