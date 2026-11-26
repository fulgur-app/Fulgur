//! Tests for the independence of the search bars of a window's tabs.

use super::SearchBar;
use crate::fulgur::ui::motion::SURFACE_DURATION;
use crate::fulgur::ui::tabs::editor_tab::replace_editor_text;
use crate::fulgur::{Fulgur, tab::TabId};
use crate::test_support::open_fulgur_with_root;
use gpui_kit::component::input::EditorState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Focusable, TestAppContext, VisualTestContext};
use std::ops::Range;

/// Open a new editor tab holding `text` and make it the active tab.
///
/// ### Arguments
/// - `fulgur`: The window entity
/// - `visual_cx`: The visual test context driving the window
/// - `text`: The content of the new tab
///
/// ### Returns
/// - `(TabId, Entity<EditorState>)`: The new tab's id and editor content
fn open_editor_tab(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    text: &str,
) -> (TabId, Entity<EditorState>) {
    let opened = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.new_tab(window, cx);
            let editor_tab = this
                .get_active_editor_tab(cx)
                .expect("expected the new tab to be an editor tab");
            let content = editor_tab.content.clone();
            let tab_id = editor_tab.id;
            content.update(cx, |content, cx| content.set_value(text, window, cx));
            (tab_id, content)
        })
    });
    visual_cx.run_until_parked();
    opened
}

/// Get the active tab's id and editor content, replacing its text.
///
/// ### Arguments
/// - `fulgur`: The window entity
/// - `visual_cx`: The visual test context driving the window
/// - `text`: The new content of the tab
///
/// ### Returns
/// - `(TabId, Entity<EditorState>)`: The active tab's id and editor content
fn fill_active_tab(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    text: &str,
) -> (TabId, Entity<EditorState>) {
    visual_cx.update(|window, cx| {
        let editor_tab = fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab");
        let opened = (editor_tab.id, editor_tab.content.clone());
        opened
            .1
            .update(cx, |content, cx| content.set_value(text, window, cx));
        opened
    })
}

/// Open the active tab's search bar and search for `query`.
///
/// ### Arguments
/// - `fulgur`: The window entity
/// - `visual_cx`: The visual test context driving the window
/// - `query`: The text to search for
///
/// ### Returns
/// - `Entity<SearchBar>`: The active tab's bar
fn search_active_tab(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
    query: &str,
) -> Entity<SearchBar> {
    let bar = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.find_in_file(window, cx));
        let bar = fulgur
            .read(cx)
            .active_search_bar(cx)
            .expect("Find must create the active tab's bar");
        bar.update(cx, |bar, cx| {
            bar.search_input
                .update(cx, |input, cx| input.set_value(query, window, cx));
            bar.perform_search(cx);
        });
        bar
    });
    visual_cx.run_until_parked();
    bar
}

/// Make a tab the active one, as a click on the tab bar does.
///
/// ### Arguments
/// - `fulgur`: The window entity
/// - `visual_cx`: The visual test context driving the window
/// - `tab_id`: The tab to activate
fn activate(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext, tab_id: TabId) {
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.activate_tab(tab_id, window, cx));
    });
    visual_cx.run_until_parked();
}

/// Read the number of matches and the current match of a bar.
///
/// ### Arguments
/// - `bar`: The search bar to inspect
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `(usize, Option<usize>)`: The match count and the current match index
fn matches_of(
    bar: &Entity<SearchBar>,
    visual_cx: &mut VisualTestContext,
) -> (usize, Option<usize>) {
    visual_cx.update(|_window, cx| {
        let bar = bar.read(cx);
        (bar.search_matches.len(), bar.current_match_index)
    })
}

/// Read the selected byte range of an editor.
///
/// ### Arguments
/// - `editor`: The editor to inspect
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `Range<usize>`: The selection, empty when only a caret is shown
fn selection_of(editor: &Entity<EditorState>, visual_cx: &mut VisualTestContext) -> Range<usize> {
    visual_cx.update(|_window, cx| editor.read(cx).selected_range())
}

/// Read the ranges painted for every match of a bar.
///
/// ### Arguments
/// - `bar`: The search bar to inspect
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `Vec<Range<usize>>`: The ranges of the non-current matches collection
fn painted_matches(
    bar: &Entity<SearchBar>,
    visual_cx: &mut VisualTestContext,
) -> Vec<Range<usize>> {
    visual_cx.update(|_window, cx| {
        bar.read(cx)
            .match_decorations
            .as_ref()
            .map_or_else(Vec::new, |decorations| decorations.all.get_ranges(cx))
    })
}

#[gpui_kit::test]
fn test_a_bar_opened_in_one_tab_is_hidden_in_the_others(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (first_tab, _) = fill_active_tab(&fulgur, &mut visual_cx, "foo bar");
    search_active_tab(&fulgur, &mut visual_cx, "foo");
    open_editor_tab(&fulgur, &mut visual_cx, "foo foo");

    let second_tab_state = visual_cx.update(|window, cx| {
        window.render_frame(cx);
        (
            fulgur.read(cx).active_search_bar(cx).is_none(),
            window.try_find("search-input").is_none(),
        )
    });
    assert_eq!(
        second_tab_state,
        (true, true),
        "the new tab must have no bar"
    );

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.open_settings(window, cx);
            this.find_in_file(window, cx);
            assert!(this.active_search_bar(cx).is_none());
        });
        window.render_frame(cx);
        assert!(
            window.try_find("search-input").is_none(),
            "Find must do nothing on the Settings tab"
        );
    });

    activate(&fulgur, &mut visual_cx, first_tab);
    visual_cx.update(|window, cx| {
        assert!(fulgur.read(cx).active_search_bar_visible(cx));
        window.render_frame(cx);
        assert!(window.try_find("search-input").is_some());
    });
}

#[gpui_kit::test]
fn test_each_tab_keeps_its_own_query_options_and_current_match(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (first_tab, _) = fill_active_tab(&fulgur, &mut visual_cx, "foo foo foo");
    let first_bar = search_active_tab(&fulgur, &mut visual_cx, "foo");
    visual_cx.update(|_window, cx| first_bar.update(cx, SearchBar::search_next));
    let (second_tab, _) = open_editor_tab(&fulgur, &mut visual_cx, "Bar bar");
    let second_bar = search_active_tab(&fulgur, &mut visual_cx, "bar");
    visual_cx.update(|_window, cx| {
        second_bar.update(cx, |bar, cx| {
            bar.match_case = true;
            bar.perform_search(cx);
        });
    });

    for tab_id in [first_tab, second_tab, first_tab] {
        activate(&fulgur, &mut visual_cx, tab_id);
    }

    assert_eq!(matches_of(&first_bar, &mut visual_cx), (3, Some(1)));
    assert_eq!(matches_of(&second_bar, &mut visual_cx), (1, Some(0)));
    visual_cx.update(|_window, cx| {
        assert_eq!(first_bar.read(cx).search_input.read(cx).value(), "foo");
        assert_eq!(second_bar.read(cx).search_input.read(cx).value(), "bar");
        assert!(!first_bar.read(cx).match_case);
        assert!(second_bar.read(cx).match_case);
    });
}

#[gpui_kit::test]
fn test_switching_tabs_keeps_the_caret_and_selection_of_the_tabs(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (first_tab, first_editor) = fill_active_tab(&fulgur, &mut visual_cx, "bar foo");
    search_active_tab(&fulgur, &mut visual_cx, "foo");
    let (second_tab, second_editor) = open_editor_tab(&fulgur, &mut visual_cx, "foo foo foo");
    search_active_tab(&fulgur, &mut visual_cx, "foo");
    visual_cx.update(|_window, cx| {
        first_editor.update(cx, |editor, cx| editor.set_selected_range(0..3, cx));
        second_editor.update(cx, |editor, cx| editor.set_selected_range(4..11, cx));
    });

    activate(&fulgur, &mut visual_cx, first_tab);
    assert_eq!(selection_of(&first_editor, &mut visual_cx), 0..3);
    activate(&fulgur, &mut visual_cx, second_tab);
    assert_eq!(selection_of(&second_editor, &mut visual_cx), 4..11);

    let editor_focused =
        visual_cx.update(|window, cx| second_editor.read(cx).focus_handle(cx).is_focused(window));
    assert!(
        editor_focused,
        "a tab switch must leave focus on the editor"
    );
}

#[gpui_kit::test]
fn test_closing_one_bar_leaves_the_other_tabs_bar_open(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (first_tab, _) = fill_active_tab(&fulgur, &mut visual_cx, "foo");
    let first_bar = search_active_tab(&fulgur, &mut visual_cx, "foo");
    open_editor_tab(&fulgur, &mut visual_cx, "foo");
    let second_bar = search_active_tab(&fulgur, &mut visual_cx, "foo");

    activate(&fulgur, &mut visual_cx, first_tab);
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("close-search-button", cx);
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        assert!(!first_bar.read(cx).is_visible());
        assert!(second_bar.read(cx).is_visible());
    });

    // A bar closing on a background tab must not move the focus away from the active tab.
    let first_search_focus = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.find_in_file(window, cx));
        first_bar.read(cx).search_input.read(cx).focus_handle(cx)
    });
    visual_cx.run_until_parked();
    visual_cx.update(|_window, cx| second_bar.update(cx, SearchBar::close));
    visual_cx.run_until_parked();
    let search_still_focused =
        visual_cx.update(|window, _cx| first_search_focus.is_focused(window));
    assert!(
        search_still_focused,
        "the active tab's search input must keep the focus"
    );
}

#[gpui_kit::test]
fn test_an_inactive_tab_edit_updates_its_matches_without_moving_its_caret(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (first_tab, first_editor) = fill_active_tab(&fulgur, &mut visual_cx, "foo bar");
    let first_bar = search_active_tab(&fulgur, &mut visual_cx, "foo");
    open_editor_tab(&fulgur, &mut visual_cx, "other");

    let caret_after_reload = visual_cx.update(|window, cx| {
        first_editor.update(cx, |editor, cx| {
            replace_editor_text(editor, "foo foo foo", window, cx);
            editor.set_selected_range(5..5, cx);
            editor.cursor()
        })
    });
    visual_cx.run_until_parked();

    assert_eq!(matches_of(&first_bar, &mut visual_cx).0, 3);
    assert_eq!(painted_matches(&first_bar, &mut visual_cx).len(), 3);
    activate(&fulgur, &mut visual_cx, first_tab);
    let caret = visual_cx.update(|_window, cx| first_editor.read(cx).cursor());
    assert_eq!(caret, caret_after_reload);
}

#[gpui_kit::test]
fn test_closing_a_tab_releases_its_search_bar(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    open_editor_tab(&fulgur, &mut visual_cx, "keep");
    let (tab_id, _) = open_editor_tab(&fulgur, &mut visual_cx, "foo");
    let bar = search_active_tab(&fulgur, &mut visual_cx, "foo").downgrade();

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| this.remove_tab_by_id(tab_id, window, cx));
    });
    visual_cx.run_until_parked();

    assert!(bar.upgrade().is_none(), "the bar must go with its tab");
}

#[gpui_kit::test]
fn test_a_theme_change_repaints_the_bars_of_inactive_tabs(cx: &mut TestAppContext) {
    use crate::fulgur::shared_state::SharedAppState;
    use gpui_kit::BorrowAppContext;

    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    fill_active_tab(&fulgur, &mut visual_cx, "foo foo");
    let first_bar = search_active_tab(&fulgur, &mut visual_cx, "foo");
    visual_cx.update(|_window, cx| first_bar.update(cx, |bar, cx| bar.clear_match_decorations(cx)));
    open_editor_tab(&fulgur, &mut visual_cx, "other");

    visual_cx.update(|_window, cx| cx.update_global::<SharedAppState, _>(|_, _| {}));
    visual_cx.run_until_parked();

    assert_eq!(
        painted_matches(&first_bar, &mut visual_cx),
        vec![0..3, 4..7]
    );
}

#[gpui_kit::test]
fn test_a_transferred_tab_keeps_its_search(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (tab_id, _) = fill_active_tab(&fulgur, &mut visual_cx, "Foo foo foo");
    let bar = search_active_tab(&fulgur, &mut visual_cx, "foo");
    visual_cx.update(|window, cx| {
        bar.update(cx, |bar, cx| {
            bar.match_case = true;
            bar.replace_input
                .update(cx, |input, cx| input.set_value("baz", window, cx));
            bar.perform_search(cx);
        });
    });

    let (caret_before, caret_after, restored) = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let data = this
                .extract_tab_transfer_data(tab_id, cx)
                .expect("expected an editor tab to transfer");
            let caret_before = data.cursor_position;
            this.pending_tab_transfer = Some(data);
            this.handle_pending_tab_transfer(window, cx);
            let caret_after = this
                .get_active_editor_tab(cx)
                .expect("expected the transferred tab to be active")
                .content
                .read(cx)
                .cursor_position();
            let restored = this
                .active_search_bar(cx)
                .expect("the transferred tab must bring its bar");
            (caret_before, caret_after, restored)
        })
    });
    visual_cx.run_until_parked();

    assert_eq!(
        caret_after, caret_before,
        "the rescan must not move the caret"
    );
    visual_cx.update(|_window, cx| {
        let restored = restored.read(cx);
        assert!(restored.is_visible());
        assert!(restored.match_case);
        assert_eq!(restored.search_input.read(cx).value(), "foo");
        assert_eq!(restored.replace_input.read(cx).value(), "baz");
        assert_eq!(restored.search_matches.len(), 2);
    });
}

#[gpui_kit::test]
fn test_a_tab_switch_shows_and_hides_the_bar_without_animating(cx: &mut TestAppContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (first_tab, _) = fill_active_tab(&fulgur, &mut visual_cx, "foo");
    search_active_tab(&fulgur, &mut visual_cx, "foo");
    let (second_tab, _) = open_editor_tab(&fulgur, &mut visual_cx, "bar");
    visual_cx.update(|window, cx| {
        cx.set_reduce_motion(false);
        window.render_frame(cx);
    });

    activate(&fulgur, &mut visual_cx, first_tab);
    let bounds_on_return = visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("search-input").bounds()
    });
    cx.executor().advance_clock(SURFACE_DURATION * 2);
    let bounds_once_settled = visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.find("search-input").bounds()
    });
    assert_eq!(
        bounds_on_return, bounds_once_settled,
        "returning to the tab must show its bar at once, without growing it"
    );

    activate(&fulgur, &mut visual_cx, second_tab);
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("search-input").is_none(),
            "leaving the tab must hide its bar at once, without collapsing it"
        );
    });
}
