use super::SearchMatch;
use super::matching::{apply_replacements, find_matches, find_matches_with_scratch};
use core::prelude::v1::test;

#[cfg(feature = "gpui-test-support")]
use super::SearchBar;
#[cfg(feature = "gpui-test-support")]
use crate::fulgur::Fulgur;
#[cfg(feature = "gpui-test-support")]
use gpui_kit::component::input::{EditorState, Undo};
#[cfg(feature = "gpui-test-support")]
use gpui_kit::{Entity, Focusable, TestAppContext, VisualTestContext};

// ========== Test helpers ==========

fn create_match(start: usize, end: usize) -> SearchMatch {
    SearchMatch { start, end }
}

#[cfg(feature = "gpui-test-support")]
#[cfg(feature = "gpui-test-support")]
use crate::test_support::{setup_fulgur, setup_fulgur_with_root};

/// Set up a `Fulgur` window and return its search bar plus the active editor's content.
#[cfg(feature = "gpui-test-support")]
fn setup_search(
    cx: &mut TestAppContext,
) -> (
    Entity<Fulgur>,
    Entity<SearchBar>,
    Entity<EditorState>,
    VisualTestContext,
) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    let (search_bar, content) = visual_cx.update(|_window, cx| {
        let this = fulgur.read(cx);
        let content = this
            .get_active_editor_tab(cx)
            .expect("expected active editor tab")
            .content
            .clone();
        (this.search_bar.clone(), content)
    });
    (fulgur, search_bar, content, visual_cx)
}

/// Set up a `Fulgur` window rooted in a `gpui_kit::component::Root`
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<SearchBar>, Entity<EditorState>, VisualTestContext)`: The window's search
///   bar, the active editor's content, and the visual context driving the window
#[cfg(feature = "gpui-test-support")]
fn setup_search_with_root(
    cx: &mut TestAppContext,
) -> (Entity<SearchBar>, Entity<EditorState>, VisualTestContext) {
    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let (search_bar, content) = visual_cx.update(|_window, cx| {
        let this = fulgur.read(cx);
        let content = this
            .get_active_editor_tab(cx)
            .expect("expected active editor tab")
            .content
            .clone();
        (this.search_bar.clone(), content)
    });
    (search_bar, content, visual_cx)
}

// ========== apply_replacements ==========

#[test]
fn test_apply_replacements_single_match() {
    let text = "Hello World";
    let matches = vec![create_match(0, 5)]; // "Hello"
    let result = apply_replacements(&matches, text, "Hi");
    assert_eq!(result, "Hi World");
}

#[test]
fn test_apply_replacements_multiple_matches() {
    let text = "hello hello hello";
    let matches = vec![
        create_match(0, 5),   // "hello"
        create_match(6, 11),  // "hello"
        create_match(12, 17), // "hello"
    ];
    let result = apply_replacements(&matches, text, "hi");
    assert_eq!(result, "hi hi hi");
}

#[test]
fn test_apply_replacements_no_matches() {
    let text = "Hello World";
    let matches = vec![];
    let result = apply_replacements(&matches, text, "Hi");
    assert_eq!(result, "Hello World");
}

#[test]
fn test_apply_replacements_match_at_start() {
    let text = "test string";
    let matches = vec![create_match(0, 4)]; // "test"
    let result = apply_replacements(&matches, text, "example");
    assert_eq!(result, "example string");
}

#[test]
fn test_apply_replacements_match_at_end() {
    let text = "test string";
    let matches = vec![create_match(5, 11)]; // "string"
    let result = apply_replacements(&matches, text, "text");
    assert_eq!(result, "test text");
}

#[test]
fn test_apply_replacements_multiline() {
    let text = "line1\nline2\nline3";
    let matches = vec![
        create_match(0, 5),   // "line1"
        create_match(6, 11),  // "line2"
        create_match(12, 17), // "line3"
    ];
    let result = apply_replacements(&matches, text, "replaced");
    assert_eq!(result, "replaced\nreplaced\nreplaced");
}

#[test]
fn test_apply_replacements_empty_replace() {
    let text = "hello world";
    let matches = vec![create_match(0, 5)]; // "hello"
    let result = apply_replacements(&matches, text, "");
    assert_eq!(result, " world");
}

#[test]
fn test_apply_replacements_non_sequential_matches() {
    let text = "hello world hello";
    let matches = vec![
        create_match(0, 5),   // "hello"
        create_match(12, 17), // "hello"
    ];
    let result = apply_replacements(&matches, text, "hi");
    assert_eq!(result, "hi world hi");
}

// ========== find_matches ==========

#[test]
fn test_find_matches_empty_query() {
    let text = "hello world";
    let matches = find_matches(text, "", false, false);
    assert_eq!(matches.len(), 0);
}

#[test]
fn test_find_matches_single_match() {
    let text = "hello world";
    let matches = find_matches(text, "world", false, false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 6);
    assert_eq!(matches[0].end, 11);
}

#[test]
fn test_find_matches_multiple_matches() {
    let text = "hello hello hello";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[1].start, 6);
    assert_eq!(matches[2].start, 12);
}

#[test]
fn test_find_matches_case_sensitive_match() {
    let text = "Hello hello HELLO";
    let matches = find_matches(text, "hello", true, false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 6); // Only lowercase "hello"
}

#[test]
fn test_find_matches_case_insensitive_match() {
    let text = "Hello hello HELLO";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 3); // All three variants
}

#[test]
fn test_find_matches_whole_word_match() {
    let text = "hello helloworld hello";
    let matches = find_matches(text, "hello", false, true);
    assert_eq!(matches.len(), 2); // Only standalone "hello", not "helloworld"
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[1].start, 17);
}

#[test]
fn test_find_matches_whole_word_with_punctuation() {
    let text = "hello, hello. hello! hello?";
    let matches = find_matches(text, "hello", false, true);
    assert_eq!(matches.len(), 4); // All match - punctuation is word boundary
}

#[test]
fn test_find_matches_whole_word_with_underscore() {
    let text = "hello hello_world _hello";
    let matches = find_matches(text, "hello", false, true);
    assert_eq!(matches.len(), 1); // Only standalone "hello", not "hello_world" or "_hello"
    assert_eq!(matches[0].start, 0);
}

#[test]
fn test_find_matches_whole_word_start_of_line() {
    let text = "hello world";
    let matches = find_matches(text, "hello", false, true);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 0);
}

#[test]
fn test_find_matches_whole_word_end_of_line() {
    let text = "world hello";
    let matches = find_matches(text, "hello", false, true);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 6);
}

#[test]
fn test_find_matches_multiline() {
    let text = "line1 hello\nline2 hello\nline3 hello";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 3);
    assert_eq!(matches[0].start, 6);
    assert_eq!(matches[1].start, 18);
    assert_eq!(matches[2].start, 30);
}

#[test]
fn test_find_matches_overlapping_not_found() {
    let text = "aaa";
    let matches = find_matches(text, "aa", false, false);
    // Should find "aa" at positions 0 and 1 (overlapping matches)
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[1].start, 1);
}

#[test]
fn test_find_matches_no_matches() {
    let text = "hello world";
    let matches = find_matches(text, "foo", false, false);
    assert_eq!(matches.len(), 0);
}

#[test]
fn test_find_matches_partial_word_match() {
    let text = "testing test retest";
    let matches = find_matches(text, "test", false, false);
    assert_eq!(matches.len(), 3); // "testing", "test", "retest" all contain "test"
}

#[test]
fn test_find_matches_partial_word_whole_word_disabled() {
    let text = "testing test retest";
    let matches = find_matches(text, "test", false, true);
    assert_eq!(matches.len(), 1); // Only standalone "test"
    assert_eq!(matches[0].start, 8);
}

#[test]
fn test_find_matches_unicode() {
    let text = "hello 世界 hello";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start, 0);
    assert_eq!(matches[1].start, 13); // byte offset, past the two 3-byte characters
}

// Note: Case-insensitive search with Unicode that changes byte length when lowercased
// (like Cyrillic) is not well-supported by the current implementation.
// The current approach of lowercasing the entire string breaks byte position tracking.
// For now, we test basic Unicode support with case-sensitive search only.

#[test]
fn test_find_matches_unicode_case_sensitive() {
    let text = "hello 世界 hello"; // Chinese characters don't change case
    let matches = find_matches(text, "hello", true, false);
    assert_eq!(matches.len(), 2);
}

#[test]
fn test_find_matches_empty_text() {
    let text = "";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 0);
}

#[test]
fn test_find_matches_query_longer_than_text() {
    let text = "hi";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 0);
}

#[test]
fn test_find_matches_special_characters() {
    let text = "hello (world) [test] {foo}";
    let matches = find_matches(text, "(world)", false, false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 6);
}

#[test]
fn test_find_matches_regex_chars_literal() {
    let text = "test.*test";
    let matches = find_matches(text, ".*", false, false);
    assert_eq!(matches.len(), 1); // Should find literal ".*", not regex
    assert_eq!(matches[0].start, 4);
}

#[test]
fn test_find_matches_whitespace() {
    let text = "hello  world   test"; // Multiple spaces
    let matches = find_matches(text, "  ", false, false);
    // Finds overlapping matches: positions 5, 12, 13
    assert_eq!(matches.len(), 3);
}

#[test]
fn test_find_matches_newlines() {
    let text = "line1\n\nline2";
    let matches = find_matches(text, "\n", false, false);
    assert_eq!(matches.len(), 2);
    assert_eq!(matches[0].start, 5);
    assert_eq!(matches[1].start, 6);
}

#[test]
fn test_find_matches_tabs() {
    let text = "hello\tworld\ttest";
    let matches = find_matches(text, "\t", false, false);
    assert_eq!(matches.len(), 2);
}

#[test]
fn test_find_matches_whole_word_numbers() {
    let text = "123 test123 456test 789";
    let matches = find_matches(text, "123", false, true);
    assert_eq!(matches.len(), 1); // Only standalone "123"
    assert_eq!(matches[0].start, 0);
}

#[test]
fn test_find_matches_single_character() {
    let text = "a b a c a";
    let matches = find_matches(text, "a", false, false);
    assert_eq!(matches.len(), 3);
}

#[test]
fn test_find_matches_single_character_whole_word() {
    let text = "a ba ca da";
    let matches = find_matches(text, "a", false, true);
    assert_eq!(matches.len(), 1); // Only standalone "a"
    assert_eq!(matches[0].start, 0);
}

#[test]
fn test_find_matches_offsets_after_a_newline() {
    let text = "line1\nline2 hello\nline3";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].start, 12);
    assert_eq!(matches[0].end, 17);
}

#[test]
fn test_find_matches_with_scratch_matches_baseline() {
    let text = "Alpha beta\nalpha BETA\nalpha";
    let query = "alpha";
    let baseline = find_matches(text, query, false, false);
    let mut lowercase_text_scratch = String::new();
    let mut lowercase_offsets_scratch = Vec::new();
    let with_scratch = find_matches_with_scratch(
        text,
        query,
        false,
        false,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    );
    assert_eq!(with_scratch.len(), baseline.len());
    assert_eq!(with_scratch[0].start, baseline[0].start);
    assert_eq!(with_scratch[1].start, baseline[1].start);
    assert_eq!(with_scratch[2].start, baseline[2].start);
}

#[test]
fn test_find_matches_with_scratch_rebuilds_offsets_between_calls() {
    let mut lowercase_text_scratch = "stale".repeat(64);
    let mut lowercase_offsets_scratch = vec![42, 43, 44];
    let first = find_matches_with_scratch(
        "line1\nline2\nline3",
        "line",
        false,
        false,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    );
    assert_eq!(first.len(), 3);

    let second = find_matches_with_scratch(
        "short",
        "sh",
        false,
        false,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].start, 0);
    assert_eq!(second[0].end, 2);
}

#[test]
fn test_find_matches_case_insensitive_offsets_after_shrinking_char() {
    // `ẞ` (U+1E9E, 3 bytes) lowercases to `ß` (U+00DF, 2 bytes), so the lowercased
    // haystack is one byte shorter before the match. Offsets must still point into the
    // original text.
    let text = "ẞ hello";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 1);
    let hello_start = text.find("hello").unwrap();
    assert_eq!(matches[0].start, hello_start);
    assert_eq!(matches[0].end, text.len());
    assert_eq!(&text[matches[0].start..matches[0].end], "hello");
}

#[test]
fn test_find_matches_case_insensitive_offsets_after_growing_char() {
    // `İ` (U+0130, 2 bytes) lowercases to `i` + combining dot (U+0069 U+0307, 3 bytes),
    // so the lowercased haystack is one byte longer before the match.
    let text = "İ world";
    let matches = find_matches(text, "WORLD", false, false);
    assert_eq!(matches.len(), 1);
    let world_start = text.find("world").unwrap();
    assert_eq!(matches[0].start, world_start);
    assert_eq!(matches[0].end, text.len());
    assert_eq!(&text[matches[0].start..matches[0].end], "world");
}

#[test]
fn test_find_matches_case_insensitive_whole_word_after_shrinking_char() {
    let text = "ẞ hello world";
    let matches = find_matches(text, "HELLO", false, true);
    assert_eq!(matches.len(), 1);
    assert_eq!(&text[matches[0].start..matches[0].end], "hello");
}

// ========== Visibility control ==========

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_search_bar_hidden_by_default(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|_window, cx| {
        assert!(!search_bar.read(cx).is_visible());
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_search_bar_visible_when_open(cx: &mut TestAppContext) {
    let (fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.find_in_file(window, cx);
        });
        assert!(search_bar.read(cx).is_visible());
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_find_and_replace_opens_the_bar_with_the_replace_input_focused(cx: &mut TestAppContext) {
    let (fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.find_and_replace(window, cx);
        });
        assert!(search_bar.read(cx).is_visible());
        assert!(
            search_bar
                .read(cx)
                .replace_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_find_and_replace_keeps_an_already_open_bar_open(cx: &mut TestAppContext) {
    let (fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.find_in_file(window, cx);
            this.find_and_replace(window, cx);
        });
        assert!(search_bar.read(cx).is_visible());
        assert!(
            search_bar
                .read(cx)
                .replace_input
                .read(cx)
                .focus_handle(cx)
                .is_focused(window)
        );
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_editor_never_opens_the_upstream_search_panel(cx: &mut TestAppContext) {
    let (fulgur, _search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        assert!(!content.read(cx).search_session().open);

        fulgur.update(cx, |this, cx| {
            this.find_in_file(window, cx);
            this.find_and_replace(window, cx);
        });
        assert!(!content.read(cx).search_session().open);

        // The upstream panel stays shut even when its own entry point is called.
        content.update(cx, |editor, cx| {
            editor.open_search(true, cx);
        });
        assert!(!content.read(cx).search_session().open);
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_open_search_sets_show_search_and_close_clears_it(cx: &mut TestAppContext) {
    let (fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        assert!(!search_bar.read(cx).is_visible());

        fulgur.update(cx, |this, cx| {
            this.find_in_file(window, cx);
        });
        assert!(search_bar.read(cx).is_visible());

        search_bar.update(cx, |bar, cx| {
            bar.close(cx);
        });
        assert!(!search_bar.read(cx).is_visible());
    });
}

// ========== Default toggle state ==========

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_search_toggle_defaults(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|_window, cx| {
        let bar = search_bar.read(cx);
        assert!(!bar.show_search);
        assert!(!bar.match_case);
        assert!(!bar.match_whole_word);
        assert!(bar.search_matches.is_empty());
        assert!(bar.current_match_index.is_none());
    });
}

// ========== Toggle state reflected in search results ==========

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_match_case_toggle_filters_results(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("Hello hello HELLO", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
            });

            bar.match_case = false;
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 3);

            bar.match_case = true;
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 1);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_match_whole_word_toggle_filters_results(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("test testing tested test", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.search_input.update(cx, |input, cx| {
                input.set_value("test", window, cx);
            });

            bar.match_whole_word = false;
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 4);

            bar.match_whole_word = true;
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 2);
        });
    });
}

// ========== Match count state ==========

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_no_match_state_when_query_not_found(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("aaa bbb ccc", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.search_input.update(cx, |input, cx| {
                input.set_value("zzz", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);

            assert!(bar.search_matches.is_empty());
            assert!(bar.current_match_index.is_none());
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_close_search_clears_match_state(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 3);
            assert!(bar.current_match_index.is_some());

            bar.close(cx);

            assert!(!bar.show_search);
            assert!(bar.search_matches.is_empty());
            assert!(bar.current_match_index.is_none());
        });
    });
}

// ========== Navigation ==========

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_search_next_previous_wrap_and_cursor(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("aaa\nbbb\nccc", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.search_matches = vec![create_match(0, 1), create_match(4, 5), create_match(8, 9)];
            bar.current_match_index = Some(2);

            bar.search_next(Some(content.clone()), cx);
            assert_eq!(bar.current_match_index, Some(0));
            let cursor = content.read(cx).cursor_position();
            assert_eq!(cursor.line, 0);
            assert_eq!(cursor.character, 0);

            bar.search_previous(Some(content.clone()), cx);
            assert_eq!(bar.current_match_index, Some(2));
            let cursor = content.read(cx).cursor_position();
            assert_eq!(cursor.line, 2);
            assert_eq!(cursor.character, 0);
        });
    });
}

// ========== Replace ==========

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_replace_current_updates_text_and_matches(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo bar foo", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.match_case = false;
            bar.match_whole_word = false;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("baz", window, cx);
            });

            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 2);
            assert_eq!(bar.current_match_index, Some(0));

            bar.replace_current(Some(content.clone()), window, cx);

            let text = content.read(cx).text().to_string();
            assert_eq!(text, "baz bar foo");
            assert_eq!(bar.search_matches.len(), 1);
            assert_eq!(bar.current_match_index, Some(0));

            let cursor = content.read(cx).cursor_position();
            assert_eq!(cursor.line, 0);
            assert_eq!(cursor.character, 8);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_replace_all_whole_word_only(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("test testing test", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.match_case = false;
            bar.match_whole_word = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("test", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("done", window, cx);
            });

            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 2);

            bar.replace_all(Some(content.clone()), window, cx);

            let text = content.read(cx).text().to_string();
            assert_eq!(text, "done testing done");
            assert!(bar.search_matches.is_empty());
            assert_eq!(bar.current_match_index, None);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_replace_all_preserves_undo_history(cx: &mut TestAppContext) {
    let (search_bar, content, mut visual_cx) = setup_search_with_root(cx);

    // Type through the editable path so the pre-replace text is itself an undoable
    // edit; seeding with `set_value` would clear the stack and make this vacuous.
    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.focus(window, cx);
            content.insert("alpha beta alpha", window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|window, cx| {
        search_bar.update(cx, |bar, cx| {
            bar.search_input.update(cx, |input, cx| {
                input.set_value("alpha", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("gamma", window, cx);
            });
            bar.replace_all(Some(content.clone()), window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        assert_eq!(
            content.read(cx).text().to_string(),
            "gamma beta gamma",
            "replace-all must rewrite every match"
        );
    });

    visual_cx.dispatch_action(Undo);
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        assert_eq!(
            content.read(cx).text().to_string(),
            "alpha beta alpha",
            "a single undo must restore the pre-replace text"
        );
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_replace_current_recomputes_after_buffer_edit(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("alpha beta alpha", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.match_case = false;
            bar.match_whole_word = false;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("alpha", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("x", window, cx);
            });

            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 2);
            // Point at the second match, whose offset (11) is about to go stale.
            bar.current_match_index = Some(1);

            // Shrink the buffer without refreshing matches; offset 11 is now out
            // of bounds. Replacing previously sliced out of bounds and panicked.
            content.update(cx, |content, cx| {
                content.set_value("alpha", window, cx);
            });

            bar.replace_current(Some(content.clone()), window, cx);

            let text = content.read(cx).text().to_string();
            assert_eq!(text, "x");
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_replace_all_recomputes_after_buffer_edit(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.match_case = false;
            bar.match_whole_word = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("baz", window, cx);
            });

            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 3);

            // Shrink the buffer without refreshing matches; offsets 4 and 8 are
            // now out of bounds. Replace All previously corrupted or panicked.
            content.update(cx, |content, cx| {
                content.set_value("foo", window, cx);
            });

            bar.replace_all(Some(content.clone()), window, cx);

            let text = content.read(cx).text().to_string();
            assert_eq!(text, "baz");
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_gpui_replace_all_case_sensitive_non_whole_word(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("aaaa", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.match_case = true;
            bar.match_whole_word = false;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("aa", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("b", window, cx);
            });

            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 3);

            bar.replace_all(Some(content.clone()), window, cx);

            let text = content.read(cx).text().to_string();
            assert_eq!(text, "bb");
            assert!(bar.search_matches.is_empty());
            assert_eq!(bar.current_match_index, None);
        });
    });
}

// ========== Match decorations ==========

/// Read the ranges painted by a search bar's two decoration collections
///
/// ### Arguments
/// - `bar`: The search bar holding the collections
/// - `content`: The editor the matches were painted in
/// - `cx`: The application context
///
/// ### Returns
/// - `(Vec<Range<usize>>, Vec<Range<usize>>)`: The non-current match ranges and
///   the current match range, both empty when nothing is painted
#[cfg(feature = "gpui-test-support")]
fn painted_ranges(
    bar: &SearchBar,
    content: &Entity<EditorState>,
    cx: &gpui_kit::App,
) -> (Vec<std::ops::Range<usize>>, Vec<std::ops::Range<usize>>) {
    match bar.match_decorations.get(&content.entity_id()) {
        Some(decorations) => (
            decorations.all.get_ranges(cx),
            decorations.current.get_ranges(cx),
        ),
        None => (Vec::new(), Vec::new()),
    }
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_search_paints_every_match_and_accents_the_current_one(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);

            assert_eq!(bar.current_match_index, Some(0));
            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(current, vec![0..3]);
            assert_eq!(all, vec![4..7, 8..11]);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_navigation_moves_the_accented_match(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);

            bar.search_next(Some(content.clone()), cx);
            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(current, vec![4..7]);
            assert_eq!(all, vec![0..3, 8..11]);

            bar.search_previous(Some(content.clone()), cx);
            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(current, vec![0..3]);
            assert_eq!(all, vec![4..7, 8..11]);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_close_search_clears_match_decorations(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 3);

            bar.close(cx);

            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(all, [] as [std::ops::Range<usize>; 0]);
            assert_eq!(current, [] as [std::ops::Range<usize>; 0]);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_clearing_the_query_clears_match_decorations(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);
            assert_eq!(bar.search_matches.len(), 3);

            bar.search_input.update(cx, |input, cx| {
                input.set_value("", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);

            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(all, [] as [std::ops::Range<usize>; 0]);
            assert_eq!(current, [] as [std::ops::Range<usize>; 0]);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_decoration_collections_are_reused_across_searches(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo bar foo bar", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            for query in ["foo", "bar", "foo"] {
                bar.search_input.update(cx, |input, cx| {
                    input.set_value(query, window, cx);
                });
                bar.perform_search(Some(content.clone()), cx);
            }

            assert_eq!(bar.match_decorations.len(), 1);
            // The intermediate "bar" search left the cursor on the first "bar",
            // so the final "foo" search resumes from the second occurrence.
            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(current, vec![8..11]);
            assert_eq!(all, vec![0..3]);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_all_clears_match_decorations(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("baz", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);
            bar.replace_all(Some(content.clone()), window, cx);

            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(all, [] as [std::ops::Range<usize>; 0]);
            assert_eq!(current, [] as [std::ops::Range<usize>; 0]);
        });
    });
}
