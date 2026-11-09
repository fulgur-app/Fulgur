use super::SearchMatch;
use super::matching::{
    apply_replacements, find_matches, find_matches_with_scratch, first_match_from,
    lowered_to_original, offset_after_replacements, rebuild_lowercase_text,
};
use core::prelude::v1::test;

#[cfg(feature = "gpui-test-support")]
use super::SearchBar;
#[cfg(feature = "gpui-test-support")]
use crate::fulgur::Fulgur;
#[cfg(feature = "gpui-test-support")]
use gpui_kit::component::input::{EditorState, InputEditsEvent, Undo};
#[cfg(feature = "gpui-test-support")]
use gpui_kit::{Entity, Focusable, Pixels, Point, TestAppContext, VisualTestContext, point, px};
#[cfg(feature = "gpui-test-support")]
use std::{cell::RefCell, rc::Rc};

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

// ========== offset_after_replacements ==========

#[test]
fn test_offset_after_replacements_stays_on_the_same_text() {
    let text = "foo a foo b foo";
    let matches = find_matches(text, "foo", true, false);
    let replaced = apply_replacements(&matches, text, "quux");
    assert_eq!(replaced, "quux a quux b quux");

    let cases = [
        (text.find("foo").unwrap(), replaced.find("quux").unwrap()),
        (text.find('a').unwrap(), replaced.find('a').unwrap()),
        (text.find('b').unwrap(), replaced.find('b').unwrap()),
    ];
    for (offset, expected) in cases {
        assert_eq!(offset_after_replacements(&matches, offset, 4), expected);
    }
}

#[test]
fn test_offset_after_replacements_handles_a_shorter_replacement() {
    let text = "foo a foo b";
    let matches = find_matches(text, "foo", true, false);
    let replaced = apply_replacements(&matches, text, "x");

    assert_eq!(
        offset_after_replacements(&matches, text.find('b').unwrap(), 1),
        replaced.find('b').unwrap()
    );
}

#[test]
fn test_offset_after_replacements_maps_the_end_of_the_text() {
    let text = "foo a foo";
    let matches = find_matches(text, "foo", true, false);
    let replaced = apply_replacements(&matches, text, "quux");

    assert_eq!(
        offset_after_replacements(&matches, text.len(), 4),
        replaced.len()
    );
}

#[test]
fn test_offset_after_replacements_moves_an_offset_inside_a_match_to_its_start() {
    let matches = vec![create_match(0, 3), create_match(6, 9)];

    assert_eq!(offset_after_replacements(&matches, 7, 4), 7);
}

#[test]
fn test_offset_after_replacements_skips_overlapping_matches_like_the_replacement_does() {
    let text = "aaaa z";
    let matches = find_matches(text, "aa", true, false);
    assert_eq!(matches.len(), 3, "the matches must overlap");
    let replaced = apply_replacements(&matches, text, "b");
    assert_eq!(replaced, "bb z");

    assert_eq!(
        offset_after_replacements(&matches, text.find('z').unwrap(), 1),
        replaced.find('z').unwrap()
    );
}

#[test]
fn test_offset_after_replacements_without_matches_keeps_the_offset() {
    assert_eq!(offset_after_replacements(&[], 5, 4), 5);
}

// ========== first_match_from ==========

#[test]
fn test_first_match_from_without_matches_is_none() {
    assert_eq!(first_match_from(&[], 3), None);
}

#[test]
fn test_first_match_from_includes_a_match_starting_at_the_offset() {
    let matches = vec![create_match(0, 3), create_match(4, 7), create_match(8, 11)];
    assert_eq!(first_match_from(&matches, 4), Some(1));
}

#[test]
fn test_first_match_from_skips_a_match_containing_the_offset() {
    let matches = vec![create_match(0, 3), create_match(4, 7), create_match(8, 11)];
    assert_eq!(first_match_from(&matches, 5), Some(2));
}

#[test]
fn test_first_match_from_wraps_past_the_last_match() {
    let matches = vec![create_match(0, 3), create_match(4, 7)];
    assert_eq!(first_match_from(&matches, 7), Some(0));
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
    let mut lowercase_text_scratch = String::new();
    let mut length_changes_scratch = Vec::new();
    let first = find_matches_with_scratch(
        "\u{1E9E}\u{1E9E} line1\nline2\nline3",
        "line",
        false,
        false,
        &mut lowercase_text_scratch,
        &mut length_changes_scratch,
    );
    assert_eq!(first.len(), 3);
    assert_eq!(first[0].start, 7);

    let second = find_matches_with_scratch(
        "short",
        "sh",
        false,
        false,
        &mut lowercase_text_scratch,
        &mut length_changes_scratch,
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].start, 0);
    assert_eq!(second[0].end, 2);
}

#[test]
fn test_lowercase_rebuild_records_nothing_when_no_offset_moves() {
    let mut lowercase_text = String::new();
    let mut length_changes = Vec::new();

    rebuild_lowercase_text("Hello WORLD", &mut lowercase_text, &mut length_changes);
    assert_eq!(lowercase_text, "hello world");
    assert!(length_changes.is_empty(), "ASCII text must not need a map");

    rebuild_lowercase_text("\u{C9}T\u{C9}", &mut lowercase_text, &mut length_changes);
    assert_eq!(lowercase_text, "\u{E9}t\u{E9}");
    assert!(
        length_changes.is_empty(),
        "non-ASCII characters keeping their byte length must not be recorded"
    );
}

#[test]
fn test_lowercase_rebuild_records_only_the_length_changing_characters() {
    let mut lowercase_text = String::new();
    let mut length_changes = Vec::new();

    let text = format!(
        "{}\u{1E9E}{}\u{130}",
        "a".repeat(1000),
        "\u{C9}".repeat(1000)
    );
    rebuild_lowercase_text(&text, &mut lowercase_text, &mut length_changes);

    assert_eq!(length_changes.len(), 2);
}

/// Build the per-byte lowercased-to-original map the sparse one has to agree with.
///
/// ### Arguments
/// - `text`: The original source text
///
/// ### Returns
/// - `Vec<usize>`: The original byte offset of every lowercased byte, plus the text end
fn dense_lowered_to_original_map(text: &str) -> Vec<usize> {
    let mut offsets = Vec::new();
    for (original_offset, ch) in text.char_indices() {
        let lowered_len: usize = ch.to_lowercase().map(char::len_utf8).sum();
        offsets.extend(std::iter::repeat_n(original_offset, lowered_len));
    }
    offsets.push(text.len());
    offsets
}

#[test]
fn test_lowered_to_original_agrees_with_a_dense_map_across_unicode() {
    let text: String = ('\0'..=char::MAX)
        .filter(|ch| !ch.to_lowercase().eq(std::iter::once(*ch)))
        .flat_map(|ch| [ch, 'x'])
        .collect();
    let dense = dense_lowered_to_original_map(&text);
    let mut lowercase_text = String::new();
    let mut length_changes = Vec::new();

    rebuild_lowercase_text(&text, &mut lowercase_text, &mut length_changes);

    assert_eq!(dense.len(), lowercase_text.len() + 1);
    for (lowered_offset, expected) in dense.iter().enumerate() {
        if lowercase_text.is_char_boundary(lowered_offset) {
            assert_eq!(
                lowered_to_original(lowered_offset, &length_changes),
                *expected,
                "lowered offset {lowered_offset} must map like the dense map"
            );
        }
    }
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

/// Run "Replace" twice on a document and return the text after each click
///
/// ### Arguments
/// - `cx`: The test application context
/// - `document`: The initial editor text, searched from its start
/// - `query`: The search query, matched case-insensitively
/// - `replacement`: The replacement text
///
/// ### Returns
/// - `(String, String)`: The editor text after the first and after the second replace
#[cfg(feature = "gpui-test-support")]
fn replace_current_twice(
    cx: &mut TestAppContext,
    document: &str,
    query: &str,
    replacement: &str,
) -> (String, String) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);
    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value(document, window, cx);
            content.set_selected_range(0..0, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.match_case = false;
            bar.match_whole_word = false;
            bar.search_input.update(cx, |input, cx| {
                input.set_value(query, window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value(replacement, window, cx);
            });
            bar.replace_current(Some(content.clone()), window, cx);
        });
        let after_first = content.read(cx).text().to_string();
        search_bar.update(cx, |bar, cx| {
            bar.replace_current(Some(content.clone()), window, cx);
        });
        let after_second = content.read(cx).text().to_string();
        (after_first, after_second)
    })
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_current_moves_past_a_case_fix_that_still_matches(cx: &mut TestAppContext) {
    let (after_first, after_second) =
        replace_current_twice(cx, "colour and colour", "colour", "Colour");

    assert_eq!(after_first, "Colour and colour");
    assert_eq!(
        after_second, "Colour and Colour",
        "the second replace must move on to the next match, not rewrite the first one"
    );
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_current_moves_past_a_replacement_containing_the_query(cx: &mut TestAppContext) {
    let (after_first, after_second) = replace_current_twice(cx, "foo foo", "foo", "foobar");

    assert_eq!(after_first, "foobar foo");
    assert_eq!(
        after_second, "foobar foobar",
        "the second replace must not grow the text it just inserted"
    );
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_current_puts_the_caret_after_the_replacement(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);
    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo foo", window, cx);
            content.set_selected_range(0..0, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("foobar", window, cx);
            });
            bar.replace_current(Some(content.clone()), window, cx);

            assert_eq!(bar.search_matches.len(), 2);
            assert_eq!(bar.current_match_index, Some(1));
        });
        assert_eq!(content.read(cx).cursor(), "foobar ".len());
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

/// Type a long document, open the search bar on it and park the caret
///
/// The query is `foo` and the replacement `quux`. The buffer is typed rather than
/// seeded with `set_value`, so it overflows the viewport and ends up scrolled.
///
/// ### Arguments
/// - `search_bar`: The search bar under test
/// - `content`: The active editor's content
/// - `visual_cx`: The visual context driving the window
/// - `document`: The text to type into the editor
/// - `caret`: The byte offset to park the caret on
///
/// ### Returns
/// - `Point<Pixels>`: The non-zero scroll offset the editor settled on
#[cfg(feature = "gpui-test-support")]
fn open_bar_on_scrolled_document(
    search_bar: &Entity<SearchBar>,
    content: &Entity<EditorState>,
    visual_cx: &mut VisualTestContext,
    document: &str,
    caret: usize,
) -> Point<Pixels> {
    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.focus(window, cx);
            content.insert(document.to_string(), window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|window, cx| {
        search_bar.update(cx, |bar, cx| {
            bar.toggle(Some(content.clone()), window, cx);
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("quux", window, cx);
            });
        });
        content.update(cx, |content, cx| {
            content.set_selected_range(caret..caret, cx);
        });
    });
    visual_cx.run_until_parked();

    let scroll = visual_cx.update(|_window, cx| content.read(cx).scroll_offset());
    assert_ne!(
        scroll,
        point(px(0.), px(0.)),
        "the test needs a non-zero scroll offset to be meaningful"
    );
    scroll
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_current_edits_only_the_match(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo bar foo", window, cx);
            content.set_selected_range(8..8, cx);
        });
    });

    let recorded_edits = Rc::new(RefCell::new(Vec::new()));
    let _edits_subscription = visual_cx.update(|_window, cx| {
        let recorded_edits = recorded_edits.clone();
        cx.subscribe(&content, move |_, event: &InputEditsEvent, _| {
            recorded_edits.borrow_mut().extend(
                event
                    .edits()
                    .iter()
                    .map(|(edit, _)| (edit.start_byte, edit.old_end_byte, edit.new_end_byte)),
            );
        })
    });

    visual_cx.update(|window, cx| {
        search_bar.update(cx, |bar, cx| {
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.replace_input.update(cx, |input, cx| {
                input.set_value("quux", window, cx);
            });
            bar.replace_current(Some(content.clone()), window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        assert_eq!(content.read(cx).text().to_string(), "foo bar quux");
    });
    assert_eq!(
        *recorded_edits.borrow(),
        vec![(8, 11, 12)],
        "a single replace must edit the match, not rewrite the document"
    );
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_current_keeps_the_view_when_the_next_match_is_visible(cx: &mut TestAppContext) {
    let (search_bar, content, mut visual_cx) = setup_search_with_root(cx);
    let filler = "filler line\n".repeat(200);
    let document = format!("{filler}foo one\nfoo two\nkeep\nkeep\nkeep\n");
    let scroll_before = open_bar_on_scrolled_document(
        &search_bar,
        &content,
        &mut visual_cx,
        &document,
        filler.len(),
    );

    visual_cx.update(|window, cx| {
        search_bar.update(cx, |bar, cx| {
            bar.replace_current(Some(content.clone()), window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        let content = content.read(cx);
        assert!(
            content
                .text()
                .to_string()
                .ends_with("quux one\nfoo two\nkeep\nkeep\nkeep\n")
        );
        assert_eq!(
            content.cursor(),
            filler.len() + "quux one\n".len(),
            "the caret must land on the next match"
        );
        assert_eq!(
            content.scroll_offset(),
            scroll_before,
            "a visible next match must not move the view"
        );
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_current_is_one_undoable_edit(cx: &mut TestAppContext) {
    let (search_bar, content, mut visual_cx) = setup_search_with_root(cx);

    // Typed so the pre-replace text is itself on the undo stack: `set_value` would
    // clear it, and a failed undo would then look like a no-op.
    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.focus(window, cx);
            content.insert("alpha beta alpha", window, cx);
            content.set_selected_range(0..0, cx);
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
            bar.replace_current(Some(content.clone()), window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        assert_eq!(content.read(cx).text().to_string(), "gamma beta alpha");
    });

    visual_cx.dispatch_action(Undo);
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        assert_eq!(content.read(cx).text().to_string(), "alpha beta alpha");
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_replace_all_keeps_the_scroll_offset_and_the_caret(cx: &mut TestAppContext) {
    let (search_bar, content, mut visual_cx) = setup_search_with_root(cx);
    let above = "foo above\n".repeat(200);
    let document = format!("{above}caret line\nfoo below\nkeep\nkeep\nkeep\n");
    let scroll_before = open_bar_on_scrolled_document(
        &search_bar,
        &content,
        &mut visual_cx,
        &document,
        above.len(),
    );

    visual_cx.update(|window, cx| {
        search_bar.update(cx, |bar, cx| {
            bar.replace_all(Some(content.clone()), window, cx);
        });
    });
    visual_cx.run_until_parked();

    visual_cx.update(|_window, cx| {
        let content = content.read(cx);
        let text = content.text().to_string();
        assert!(!text.contains("foo"), "every match must be replaced");
        assert_eq!(
            Some(content.cursor()),
            text.find("caret line"),
            "the caret must stay on the same text despite the length changes above it"
        );
        assert_eq!(
            content.scroll_offset(),
            scroll_before,
            "Replace All must not move the view"
        );
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
            assert_eq!(all, vec![0..3, 4..7, 8..11]);
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
            assert_eq!(all, vec![0..3, 4..7, 8..11]);

            bar.search_previous(Some(content.clone()), cx);
            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(current, vec![0..3]);
            assert_eq!(all, vec![0..3, 4..7, 8..11]);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_navigation_rewrites_only_the_current_match_decoration(cx: &mut TestAppContext) {
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
            bar.match_decorations[&content.entity_id()].all.clear(cx);

            bar.search_next(Some(content.clone()), cx);
            bar.search_previous(Some(content.clone()), cx);

            let (all, current) = painted_ranges(bar, &content, cx);
            assert_eq!(
                all,
                [] as [std::ops::Range<usize>; 0],
                "stepping must not rebuild the list of every match"
            );
            assert_eq!(current, vec![0..3]);
        });
    });
}

/// Read a decoration collection's creation rank from its debug form.
///
/// ### Arguments
/// - `collection`: The collection to inspect
///
/// ### Returns
/// - `usize`: The collection's creation rank within its editor
#[cfg(feature = "gpui-test-support")]
fn decoration_collection_rank(
    collection: &gpui_kit::component::input::TextDecorationCollection,
) -> usize {
    let debug = format!("{collection:?}");
    let (_, after_id) = debug
        .rsplit_once("DecorationCollectionId(")
        .expect("expected the collection id in the debug form");
    let (rank, _) = after_id
        .split_once(')')
        .expect("expected the collection id to close");
    rank.parse().expect("expected a numeric collection id")
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_current_match_collection_outranks_the_all_matches_collection(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);

            let decorations = &bar.match_decorations[&content.entity_id()];
            assert!(
                decoration_collection_rank(&decorations.current)
                    < decoration_collection_rank(&decorations.all),
                "the first collection wins an overlap, so the accent must be created first"
            );
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_close_frees_the_scan_buffers(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("\u{1E9E} FOO foo", window, cx);
        });
        search_bar.update(cx, |bar, cx| {
            bar.show_search = true;
            bar.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            bar.perform_search(Some(content.clone()), cx);
            assert!(bar.search_text_scratch.capacity() > 0);
            assert!(bar.search_lowercase_text_scratch.capacity() > 0);
            assert!(bar.search_length_changes_scratch.capacity() > 0);
            assert!(bar.search_matches.capacity() > 0);

            bar.close(cx);

            assert_eq!(bar.search_text_scratch.capacity(), 0);
            assert_eq!(bar.search_lowercase_text_scratch.capacity(), 0);
            assert_eq!(bar.search_length_changes_scratch.capacity(), 0);
            assert_eq!(bar.search_matches.capacity(), 0);
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
            assert_eq!(all, vec![0..3, 8..11]);
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
