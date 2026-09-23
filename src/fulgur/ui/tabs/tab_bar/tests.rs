use super::state::TabTooltipContent;
use super::{TabBar, TabBarEvent};
use crate::fulgur::Fulgur;
use crate::fulgur::sync::share::ShareOrigin;
use crate::fulgur::sync::ssh::url::RemoteSpec;
use crate::fulgur::ui::components_utils::format_local_datetime;
use crate::fulgur::ui::tabs::editor_tab::TabLocation;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};
use std::path::PathBuf;
use time::macros::datetime;

use crate::test_support::setup_fulgur;

// ========== get_tab_display_title tests ==========

#[gpui_kit::test]
fn test_get_tab_display_title_returns_filename_for_unique_path(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(e) = tab.as_editor_mut() {
                        e.location = TabLocation::Local(PathBuf::from("/projects/foo/main.rs"));
                    }
                });
            let filename_counts = TabBar::build_tab_filename_counts(&this.tabs, cx);
            let tab = this
                .tabs
                .first()
                .expect("expected at least one tab")
                .read(cx);
            let (filename, folder) = TabBar::get_tab_display_title(tab, &filename_counts);
            assert_eq!(filename, "main.rs");
            assert!(
                folder.is_none(),
                "unique filename should have no parent folder suffix"
            );
        });
    });
}

#[gpui_kit::test]
fn test_get_tab_display_title_shows_parent_folder_for_duplicate_filenames(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(e) = tab.as_editor_mut() {
                        e.location = TabLocation::Local(PathBuf::from("/projects/a/main.rs"));
                    }
                });
            this.new_tab(window, cx);
            this.tabs[1].clone().update(cx, |tab, _cx| {
                if let Some(e) = tab.as_editor_mut() {
                    e.location = TabLocation::Local(PathBuf::from("/projects/b/main.rs"));
                }
            });
            let filename_counts = TabBar::build_tab_filename_counts(&this.tabs, cx);
            let tab0 = this
                .tabs
                .first()
                .expect("expected at least one tab")
                .read(cx);
            let (filename0, folder0) = TabBar::get_tab_display_title(tab0, &filename_counts);
            assert_eq!(filename0, "main.rs");
            assert_eq!(
                folder0.as_deref(),
                Some("../a"),
                "first tab should show its parent folder when filename is shared"
            );
            let tab1 = this.tabs.get(1).expect("expected second tab").read(cx);
            let (filename1, folder1) = TabBar::get_tab_display_title(tab1, &filename_counts);
            assert_eq!(filename1, "main.rs");
            assert_eq!(
                folder1.as_deref(),
                Some("../b"),
                "second tab should show its own parent folder"
            );
        });
    });
}

#[gpui_kit::test]
fn test_get_tab_display_title_returns_tab_title_for_untitled_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            // The default tab has no file_path; its display title should be the tab's own title
            let filename_counts = TabBar::build_tab_filename_counts(&this.tabs, cx);
            let tab = this
                .tabs
                .first()
                .expect("expected at least one tab")
                .read(cx);
            let tab_title = tab.title().to_string();
            let (display_title, folder) = TabBar::get_tab_display_title(tab, &filename_counts);
            assert_eq!(display_title, tab_title);
            assert!(folder.is_none());
        });
    });
}

#[gpui_kit::test]
fn test_remote_tab_indicator_label_returns_ssh_for_remote_editor_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(e) = tab.as_editor_mut() {
                        e.location = TabLocation::Remote(RemoteSpec {
                            host: "example.com".to_string(),
                            port: 22,
                            user: Some("alice".to_string()),
                            path: "/tmp/test.txt".to_string(),
                            password_in_url: None,
                        });
                    }
                });
            let tab = this
                .tabs
                .first()
                .expect("default tab should exist")
                .read(cx);
            assert_eq!(TabBar::remote_tab_indicator_label(tab), Some("R"));
        });
    });
}

#[gpui_kit::test]
fn test_remote_tab_indicator_label_is_none_for_local_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.tabs
                .first()
                .expect("expected at least one tab")
                .clone()
                .update(cx, |tab, _cx| {
                    if let Some(e) = tab.as_editor_mut() {
                        e.location = TabLocation::Local(PathBuf::from("/tmp/local.txt"));
                    }
                });
            let tab = this
                .tabs
                .first()
                .expect("default tab should exist")
                .read(cx);
            assert_eq!(TabBar::remote_tab_indicator_label(tab), None);
        });
    });
}

// ========== tooltip tests ==========

/// Relocate the first tab, then build its tooltip content.
fn tooltip_for_location(
    fulgur: &Entity<Fulgur>,
    cx: &mut VisualTestContext,
    location: TabLocation,
) -> Option<TabTooltipContent> {
    cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            let tab = this
                .tabs
                .first()
                .expect("expected at least one tab")
                .clone();
            tab.update(cx, |tab, _cx| {
                let editor_tab = tab.as_editor_mut().expect("expected an editor tab");
                editor_tab.title = "received.md".into();
                editor_tab.location = location;
            });
            TabBar::tab_tooltip_content(tab.read(cx))
        })
    })
}

#[gpui_kit::test]
fn test_tab_tooltip_describes_a_received_share(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let shared_at = datetime!(2026-09-22 12:30:00 UTC);
    let origin = ShareOrigin {
        source_device_name: Some("Work laptop".to_string()),
        shared_at: Some(shared_at),
        size_bytes: 2048,
    };

    let content = tooltip_for_location(&fulgur, &mut visual_cx, TabLocation::Shared(origin));

    let shared_on = format_local_datetime(shared_at).expect("format share date");
    assert_eq!(
        content,
        Some(TabTooltipContent {
            header: "received.md".to_string(),
            details: vec![
                "Size: 2.0 KB".to_string(),
                "Shared by: Work laptop".to_string(),
                format!("Shared on: {shared_on}"),
            ],
        })
    );
}

#[gpui_kit::test]
fn test_tab_tooltip_for_a_share_without_metadata_names_an_unknown_device(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let origin = ShareOrigin {
        source_device_name: None,
        shared_at: None,
        size_bytes: 5,
    };

    let content = tooltip_for_location(&fulgur, &mut visual_cx, TabLocation::Shared(origin))
        .expect("a received share must have a tooltip");

    assert_eq!(
        content.details,
        vec![
            "Size: 5 B".to_string(),
            "Shared by: Unknown device".to_string()
        ]
    );
}

#[gpui_kit::test]
fn test_tab_tooltip_shows_the_path_once_a_share_is_saved(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let content = tooltip_for_location(
        &fulgur,
        &mut visual_cx,
        TabLocation::Local(PathBuf::from("/tmp/received.md")),
    )
    .expect("a local file tab must have a tooltip");

    assert_eq!(content.header, "/tmp/received.md");
    assert!(
        content
            .details
            .iter()
            .all(|detail| !detail.starts_with("Shared")),
        "a saved file must show the regular file details, got {:?}",
        content.details
    );
}

#[gpui_kit::test]
fn test_tab_tooltip_is_none_for_an_untitled_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    assert_eq!(
        tooltip_for_location(&fulgur, &mut visual_cx, TabLocation::Untitled),
        None
    );
}

// ========== event routing tests ==========

#[gpui_kit::test]
fn test_tab_bar_events_are_routed_to_the_window(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let (tab_bar, initial_tab_count) = visual_cx.update(|_window, cx| {
        let this = fulgur.read(cx);
        (this.tab_bar.clone(), this.tabs.len())
    });
    visual_cx.update(|_window, cx| {
        tab_bar.update(cx, |_, cx| cx.emit(TabBarEvent::NewTab));
    });
    visual_cx.run_until_parked();

    let after = visual_cx.update(|_window, cx| fulgur.read(cx).tabs.len());
    assert_eq!(after, initial_tab_count + 1);
}

#[gpui_kit::test]
fn test_tab_bar_activate_event_switches_active_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    let (tab_bar, first_tab_id) = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            let first_tab_id = this
                .tabs
                .first()
                .expect("expected at least one tab")
                .read(cx)
                .id();
            (this.tab_bar.clone(), first_tab_id)
        })
    });
    visual_cx.update(|_window, cx| {
        tab_bar.update(cx, |_, cx| cx.emit(TabBarEvent::Activate(first_tab_id)));
    });
    visual_cx.run_until_parked();

    let active = visual_cx.update(|_window, cx| fulgur.read(cx).active_tab_id);
    assert_eq!(active, Some(first_tab_id));
}

// ========== on_next_tab tests ==========

#[gpui_kit::test]
fn test_on_next_tab_advances_active_index_by_one(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            // Three tabs: move to index 0, then advance
            this.set_active_tab(0, window, cx);
            this.on_next_tab(window, cx);
            assert_eq!(this.active_tab_index(cx), Some(1));
        });
    });
}

#[gpui_kit::test]
fn test_on_next_tab_wraps_around_from_last_to_first(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            let last = this.tabs.len() - 1;
            this.set_active_tab(last, window, cx);
            this.on_next_tab(window, cx);
            assert_eq!(this.active_tab_index(cx), Some(0));
        });
    });
}

#[gpui_kit::test]
fn test_on_next_tab_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.active_tab_id = None;
            this.on_next_tab(window, cx);
            assert_eq!(this.active_tab_index(cx), None);
        });
    });
}

// ========== on_previous_tab tests ==========

#[gpui_kit::test]
fn test_on_previous_tab_moves_to_previous_index(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            let last = this.tabs.len() - 1;
            this.set_active_tab(last, window, cx);
            this.on_previous_tab(window, cx);
            assert_eq!(this.active_tab_index(cx), Some(last - 1));
        });
    });
}

#[gpui_kit::test]
fn test_on_previous_tab_wraps_around_from_first_to_last(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            this.new_tab(window, cx);
            this.set_active_tab(0, window, cx);
            this.on_previous_tab(window, cx);
            let last = this.tabs.len() - 1;
            assert_eq!(this.active_tab_index(cx), Some(last));
        });
    });
}

#[gpui_kit::test]
fn test_on_previous_tab_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.active_tab_id = None;
            this.on_previous_tab(window, cx);
            assert_eq!(this.active_tab_index(cx), None);
        });
    });
}
