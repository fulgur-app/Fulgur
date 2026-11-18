//! Headless interaction tests for the tab bar.

use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::window_manager::WindowManager;
use crate::test_support::{
    open_fulgur_with_root, open_rendered_window, register_window_in_global_manager,
};
use gpui_kit::test::{ScopedWindow, TestWindowExt};
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

#[gpui_kit::test]
fn test_activating_a_tab_overrides_an_older_pending_scroll(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let tab_ids = open_tabs(&fulgur, &mut visual_cx, 40);
    let first = tab_ids[0];
    let last_index = tab_ids.len() - 1;

    // As at startup: the restored tab's scroll is still pending when a file opened
    // from outside activates another tab.
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.request_tab_scroll(first, cx);
            this.set_active_tab(last_index, window, cx);
        });
    });
    for _ in 0..3 {
        visual_cx.update(TestWindowExt::render_frame);
        visual_cx.run_until_parked();
    }

    let offset_x =
        visual_cx.update(|_window, cx| fulgur.read(cx).tab_bar.read(cx).scroll_handle.offset().x);
    assert!(
        offset_x < gpui_kit::px(0.),
        "the tab bar must scroll to the activated last tab, not the older pending one"
    );
}

/// Find the index of the menu item carrying `label` inside a scope.
///
/// ### Arguments
/// - `scope`: The scoped window holding exactly one open menu
/// - `label`: The accessibility label of the wanted item
///
/// ### Returns
/// - `usize`: The item's element id
fn menu_item_index(scope: &ScopedWindow<'_>, label: &str) -> usize {
    (0..MAX_MENU_ITEMS)
        .find(|index| {
            scope
                .try_find(*index)
                .is_some_and(|item| item.label() == Some(label))
        })
        .unwrap_or_else(|| panic!("no menu item labelled {label:?}"))
}

/// Upper bound on the items a context menu holds, used when scanning it by label.
const MAX_MENU_ITEMS: usize = 32;

#[gpui_kit::test]
fn test_send_to_moves_the_tab_into_the_other_window(cx: &mut TestAppContext) {
    let (source, source_handle, mut source_cx) = open_fulgur_with_root(cx);
    register_window_in_global_manager(cx, source_handle.window_id(), &source);
    let (target, target_handle, mut target_cx) = open_rendered_window(cx);
    register_window_in_global_manager(cx, target_handle.window_id(), &target);
    let target_name = cx.update(|cx| {
        cx.global::<WindowManager>()
            .get_window_name(target_handle.window_id())
            .expect("the target window must be named")
            .to_string()
    });

    let tab_ids = open_tabs(&source, &mut source_cx, 1);
    let sent = tab_ids[1];
    source_cx.update(|window, cx| {
        let content = source
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .clone();
        content.update(cx, |content, cx| content.set_value("sent text", window, cx));
    });
    source_cx.run_until_parked();

    source_cx.update(|window, cx| {
        window.render_frame(cx);
        window.right_click(("tab", sent.0), cx);
    });
    source_cx.run_until_parked();
    source_cx.update(|window, cx| {
        window.render_frame(cx);
        let mut menu = window.within(("tab", sent.0));
        let send_to = menu_item_index(&menu, "Send to...");
        menu.hover(send_to, cx);
    });
    source_cx.run_until_parked();
    source_cx.update(|window, cx| {
        window.render_frame(cx);
        let mut submenu = window.within("submenu");
        let destination = menu_item_index(&submenu, &format!("Window {target_name}"));
        submenu.click(destination, cx);
    });
    source_cx.run_until_parked();
    target_cx.update(TestWindowExt::render_frame);
    source_cx.update(TestWindowExt::render_frame);
    target_cx.run_until_parked();
    source_cx.run_until_parked();

    let source_ids: Vec<TabId> = source_cx.update(|_window, cx| {
        source
            .read(cx)
            .tabs
            .iter()
            .map(|tab| tab.read(cx).id())
            .collect()
    });
    assert_eq!(
        source_ids,
        vec![tab_ids[0]],
        "the sent tab must leave the source window"
    );
    let received = target_cx.update(|_window, cx| {
        let target = target.read(cx);
        (
            target.tabs.len(),
            target
                .get_active_editor_tab(cx)
                .map(|tab| tab.content.read(cx).value().to_string()),
        )
    });
    assert_eq!(
        received,
        (2, Some("sent text".to_string())),
        "the target window must gain the tab, with its buffer, as its active tab"
    );
}
