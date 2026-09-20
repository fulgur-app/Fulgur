//! Headless interaction tests for the tab bar.

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::test_support::open_fulgur_with_root;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

/// Open `count` extra tabs and return every tab id in bar order.
///
/// ### Arguments
/// - `fulgur`: The Fulgur entity under test
/// - `visual_cx`: The visual test context driving the window
/// - `count`: How many tabs to add beyond the one the window starts with
///
/// ### Returns
/// - `Vec<TabId>`: The ids of all open tabs, left to right
fn open_tabs(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    count: usize,
) -> Vec<TabId> {
    let ids = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            for _ in 0..count {
                this.new_tab(window, cx);
            }
            this.tabs.iter().map(|tab| tab.read(cx).id()).collect()
        })
    });
    visual_cx.run_until_parked();
    ids
}

#[gpui_kit::test]
fn test_clicking_a_tab_activates_it(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let tab_ids = open_tabs(&fulgur, &mut visual_cx, 2);
    let first = tab_ids[0];
    let last = *tab_ids.last().expect("expected at least one tab");

    let active = visual_cx.update(|_window, cx| fulgur.read(cx).active_tab_id);
    assert_eq!(active, Some(last), "a new tab becomes the active tab");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(("tab", first.0), cx);
    });
    visual_cx.run_until_parked();

    let active = visual_cx.update(|_window, cx| fulgur.read(cx).active_tab_id);
    assert_eq!(active, Some(first), "clicking a tab must activate it");
}

#[gpui_kit::test]
fn test_tab_reports_its_selected_state_to_accessibility(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let tab_ids = open_tabs(&fulgur, &mut visual_cx, 1);
    let (first, second) = (tab_ids[0], tab_ids[1]);

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(window.find(("tab", first.0)).selected(), Some(false));
        assert_eq!(window.find(("tab", second.0)).selected(), Some(true));
    });

    visual_cx.update(|window, cx| {
        window.click(("tab", first.0), cx);
    });
    visual_cx.run_until_parked();

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        assert_eq!(
            window.find(("tab", first.0)).selected(),
            Some(true),
            "the clicked tab must report itself selected"
        );
        assert_eq!(window.find(("tab", second.0)).selected(), Some(false));
    });
}

#[gpui_kit::test]
fn test_clicking_the_close_button_closes_only_that_tab(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let tab_ids = open_tabs(&fulgur, &mut visual_cx, 2);
    let middle = tab_ids[1];

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(("close-tab", middle.0), cx);
    });
    visual_cx.run_until_parked();

    let remaining: Vec<TabId> = visual_cx.update(|_window, cx| {
        fulgur
            .read(cx)
            .tabs
            .iter()
            .map(|tab| tab.read(cx).id())
            .collect()
    });
    assert_eq!(remaining, vec![tab_ids[0], tab_ids[2]]);

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find(("tab", middle.0)).is_none(),
            "the closed tab must leave the render tree"
        );
    });
}

#[gpui_kit::test]
fn test_dragging_a_tab_onto_another_reorders_the_bar(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let tab_ids = open_tabs(&fulgur, &mut visual_cx, 2);
    let (first, last) = (tab_ids[0], tab_ids[2]);

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.drag_to(("tab", first.0), ("tab", last.0), cx);
    });
    visual_cx.run_until_parked();

    let order: Vec<TabId> = visual_cx.update(|_window, cx| {
        fulgur
            .read(cx)
            .tabs
            .iter()
            .map(|tab| tab.read(cx).id())
            .collect()
    });

    let moved_to = order
        .iter()
        .position(|id| *id == first)
        .expect("the dragged tab must still be open");
    assert!(
        moved_to > 0,
        "dragging the first tab towards the last must move it right, got {order:?}"
    );
    let mut sorted = order.clone();
    sorted.sort_unstable_by_key(|id| id.0);
    assert_eq!(
        sorted, tab_ids,
        "reordering must preserve exactly the tabs that were open"
    );
    assert!(
        order.contains(&last),
        "the drop target must survive the reorder"
    );
}
