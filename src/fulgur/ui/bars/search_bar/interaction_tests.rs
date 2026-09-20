//! Headless interaction tests for the search and replace bar.

use super::SearchBar;
use crate::test_support::open_fulgur_with_root;
use gpui_kit::component::input::EditorState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, TestAppContext, VisualTestContext};

/// Open a window with the search bar visible over an editor holding `content`.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `content`: The text to place in the active editor before searching
///
/// ### Returns
/// - `(Entity<SearchBar>, Entity<EditorState>, VisualTestContext)`: The window's search bar, the
///   active editor's content and the visual test context
fn setup_visible_search(
    cx: &mut TestAppContext,
    content: &str,
) -> (Entity<SearchBar>, Entity<EditorState>, VisualTestContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let (search_bar, editor) = visual_cx.update(|window, cx| {
        let editor = fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .clone();
        editor.update(cx, |editor, cx| {
            editor.set_value(content, window, cx);
        });
        fulgur.update(cx, |this, cx| this.find_in_file(window, cx));
        (fulgur.read(cx).search_bar.clone(), editor)
    });
    visual_cx.run_until_parked();
    (search_bar, editor, visual_cx)
}

/// Read how many matches the bar currently holds.
///
/// ### Arguments
/// - `search_bar`: The search bar under test
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `usize`: The number of matches from the last search
fn match_count(search_bar: &Entity<SearchBar>, visual_cx: &mut VisualTestContext) -> usize {
    visual_cx.update(|_window, cx| search_bar.read(cx).search_matches.len())
}

#[gpui_kit::test]
fn test_typing_in_the_search_input_runs_the_search(cx: &mut TestAppContext) {
    let (search_bar, _editor, mut visual_cx) = setup_visible_search(cx, "foo bar foo baz foo");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-input", cx);
        window.input("foo", cx);
    });
    visual_cx.run_until_parked();

    assert_eq!(
        match_count(&search_bar, &mut visual_cx),
        3,
        "typing a query must run the search without any extra action"
    );
    let value = visual_cx.update(|_window, cx| {
        search_bar
            .read(cx)
            .search_input
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(value, "foo");
}

#[gpui_kit::test]
fn test_clicking_next_and_previous_cycles_through_matches(cx: &mut TestAppContext) {
    let (search_bar, _editor, mut visual_cx) = setup_visible_search(cx, "foo bar foo baz foo");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-input", cx);
        window.input("foo", cx);
    });
    visual_cx.run_until_parked();

    let current = |visual_cx: &mut VisualTestContext| {
        visual_cx.update(|_window, cx| search_bar.read(cx).current_match_index)
    };
    let first = current(&mut visual_cx);

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-next-button", cx);
    });
    visual_cx.run_until_parked();
    let after_next = current(&mut visual_cx);
    assert_ne!(
        after_next, first,
        "Next must move the current match forward"
    );

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-previous-button", cx);
    });
    visual_cx.run_until_parked();
    assert_eq!(
        current(&mut visual_cx),
        first,
        "Previous must undo what Next did"
    );
}

#[gpui_kit::test]
fn test_clicking_match_case_reruns_the_search(cx: &mut TestAppContext) {
    let (search_bar, _editor, mut visual_cx) = setup_visible_search(cx, "Hello hello HELLO");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-input", cx);
        window.input("hello", cx);
    });
    visual_cx.run_until_parked();
    assert_eq!(
        match_count(&search_bar, &mut visual_cx),
        3,
        "a case insensitive search must match every casing"
    );

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("match-case-button", cx);
    });
    visual_cx.run_until_parked();

    assert!(
        visual_cx.update(|_window, cx| search_bar.read(cx).match_case),
        "clicking the toggle must flip match_case"
    );
    assert_eq!(
        match_count(&search_bar, &mut visual_cx),
        1,
        "the toggle must re-run the search, not just record the flag"
    );
}

#[gpui_kit::test]
fn test_clicking_match_whole_word_reruns_the_search(cx: &mut TestAppContext) {
    let (search_bar, _editor, mut visual_cx) = setup_visible_search(cx, "test testing tested test");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-input", cx);
        window.input("test", cx);
    });
    visual_cx.run_until_parked();
    assert_eq!(match_count(&search_bar, &mut visual_cx), 4);

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("match-whole-word-button", cx);
    });
    visual_cx.run_until_parked();

    assert_eq!(
        match_count(&search_bar, &mut visual_cx),
        2,
        "whole word must drop the substring matches"
    );
}

#[gpui_kit::test]
fn test_clicking_replace_all_rewrites_the_editor(cx: &mut TestAppContext) {
    let (_search_bar, editor, mut visual_cx) = setup_visible_search(cx, "foo bar foo baz foo");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("search-input", cx);
        window.input("foo", cx);
    });
    visual_cx.run_until_parked();

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("replace-input", cx);
        window.input("qux", cx);
    });
    visual_cx.run_until_parked();

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("replace-all-button", cx);
    });
    visual_cx.run_until_parked();

    let text = visual_cx.update(|_window, cx| editor.read(cx).value().to_string());
    assert_eq!(text, "qux bar qux baz qux");
}

#[gpui_kit::test]
fn test_clicking_close_hides_the_search_bar(cx: &mut TestAppContext) {
    let (search_bar, _editor, mut visual_cx) = setup_visible_search(cx, "foo bar foo");

    assert!(
        visual_cx.update(|_window, cx| search_bar.read(cx).is_visible()),
        "the bar must start visible"
    );

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("close-search-button", cx);
    });
    visual_cx.run_until_parked();

    assert!(
        !visual_cx.update(|_window, cx| search_bar.read(cx).is_visible()),
        "clicking close must hide the bar"
    );
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        assert!(
            window.try_find("search-input").is_none(),
            "the hidden bar must leave the render tree"
        );
    });
}
