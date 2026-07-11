use super::SearchMatch;
use super::actions::{
    apply_replacements, find_matches, find_matches_with_scratch, get_line_col_fast,
};
use core::prelude::v1::test;

#[cfg(feature = "gpui-test-support")]
use super::SearchBar;
#[cfg(feature = "gpui-test-support")]
use crate::fulgur::{
    Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
};
#[cfg(feature = "gpui-test-support")]
use gpui::{
    AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    WindowOptions, div,
};
#[cfg(feature = "gpui-test-support")]
use gpui_component::input::InputState;
#[cfg(feature = "gpui-test-support")]
use parking_lot::Mutex;
#[cfg(feature = "gpui-test-support")]
use std::{cell::RefCell, path::PathBuf, sync::Arc};

// ========== Test helpers ==========

fn create_match(start: usize, end: usize, line: usize, col: usize) -> SearchMatch {
    SearchMatch {
        start,
        end,
        line,
        col,
    }
}

fn newline_offsets(text: &str) -> Vec<usize> {
    text.bytes()
        .enumerate()
        .filter_map(|(i, b)| if b == b'\n' { Some(i) } else { None })
        .collect()
}

fn get_line_col(text: &str, byte_pos: usize) -> (usize, usize) {
    let offsets = newline_offsets(text);
    get_line_col_fast(text, byte_pos, &offsets)
}

#[cfg(feature = "gpui-test-support")]
struct EmptyView;

#[cfg(feature = "gpui-test-support")]
impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

#[cfg(feature = "gpui-test-support")]
fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files, None));
        cx.set_global(WindowManager::new());
    });

    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                *fulgur_slot.borrow_mut() = Some(fulgur);
                cx.new(|_| EmptyView)
            })
        })
        .expect("failed to open test window");

    let visual_cx = VisualTestContext::from_window(window.into(), cx);
    visual_cx.run_until_parked();
    let fulgur = fulgur_slot
        .into_inner()
        .expect("failed to capture Fulgur entity");
    (fulgur, visual_cx)
}

/// Set up a `Fulgur` window and return its search bar plus the active editor's content.
#[cfg(feature = "gpui-test-support")]
fn setup_search(
    cx: &mut TestAppContext,
) -> (
    Entity<Fulgur>,
    Entity<SearchBar>,
    Entity<InputState>,
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

// ========== apply_replacements ==========

#[test]
fn test_apply_replacements_single_match() {
    let text = "Hello World";
    let matches = vec![create_match(0, 5, 0, 0)]; // "Hello"
    let result = apply_replacements(&matches, text, "Hi");
    assert_eq!(result, "Hi World");
}

#[test]
fn test_apply_replacements_multiple_matches() {
    let text = "hello hello hello";
    let matches = vec![
        create_match(0, 5, 0, 0),    // "hello"
        create_match(6, 11, 0, 6),   // "hello"
        create_match(12, 17, 0, 12), // "hello"
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
    let matches = vec![create_match(0, 4, 0, 0)]; // "test"
    let result = apply_replacements(&matches, text, "example");
    assert_eq!(result, "example string");
}

#[test]
fn test_apply_replacements_match_at_end() {
    let text = "test string";
    let matches = vec![create_match(5, 11, 0, 5)]; // "string"
    let result = apply_replacements(&matches, text, "text");
    assert_eq!(result, "test text");
}

#[test]
fn test_apply_replacements_multiline() {
    let text = "line1\nline2\nline3";
    let matches = vec![
        create_match(0, 5, 0, 0),   // "line1"
        create_match(6, 11, 1, 0),  // "line2"
        create_match(12, 17, 2, 0), // "line3"
    ];
    let result = apply_replacements(&matches, text, "replaced");
    assert_eq!(result, "replaced\nreplaced\nreplaced");
}

#[test]
fn test_apply_replacements_empty_replace() {
    let text = "hello world";
    let matches = vec![create_match(0, 5, 0, 0)]; // "hello"
    let result = apply_replacements(&matches, text, "");
    assert_eq!(result, " world");
}

#[test]
fn test_apply_replacements_non_sequential_matches() {
    let text = "hello world hello";
    let matches = vec![
        create_match(0, 5, 0, 0),    // "hello"
        create_match(12, 17, 0, 12), // "hello"
    ];
    let result = apply_replacements(&matches, text, "hi");
    assert_eq!(result, "hi world hi");
}

// ========== get_line_col_fast ==========

#[test]
fn test_get_line_col_start_of_text() {
    let text = "hello world";
    assert_eq!(get_line_col(text, 0), (0, 0));
}

#[test]
fn test_get_line_col_middle_of_first_line() {
    let text = "hello world";
    assert_eq!(get_line_col(text, 6), (0, 6)); // 'w' in "world"
}

#[test]
fn test_get_line_col_end_of_first_line() {
    let text = "hello world";
    assert_eq!(get_line_col(text, 11), (0, 11)); // end of line
}

#[test]
fn test_get_line_col_start_of_second_line() {
    let text = "hello\nworld";
    assert_eq!(get_line_col(text, 6), (1, 0)); // 'w' in "world"
}

#[test]
fn test_get_line_col_middle_of_second_line() {
    let text = "hello\nworld";
    assert_eq!(get_line_col(text, 9), (1, 3)); // 'l' in "world"
}

#[test]
fn test_get_line_col_multiple_lines() {
    let text = "line1\nline2\nline3";
    assert_eq!(get_line_col(text, 0), (0, 0)); // 'l' in "line1"
    assert_eq!(get_line_col(text, 6), (1, 0)); // 'l' in "line2"
    assert_eq!(get_line_col(text, 12), (2, 0)); // 'l' in "line3"
}

#[test]
fn test_get_line_col_after_newline() {
    let text = "hello\n\nworld";
    assert_eq!(get_line_col(text, 6), (1, 0)); // empty line
    assert_eq!(get_line_col(text, 7), (2, 0)); // 'w' in "world"
}

#[test]
fn test_get_line_col_empty_text() {
    let text = "";
    assert_eq!(get_line_col(text, 0), (0, 0));
}

#[test]
fn test_get_line_col_windows_line_endings() {
    let text = "hello\r\nworld";
    assert_eq!(get_line_col(text, 5), (0, 5)); // '\r'
    assert_eq!(get_line_col(text, 7), (1, 0)); // 'w' in "world"
}

#[test]
fn test_get_line_col_mixed_line_endings() {
    let text = "line1\nline2\r\nline3";
    assert_eq!(get_line_col(text, 0), (0, 0)); // 'l' in "line1"
    assert_eq!(get_line_col(text, 6), (1, 0)); // 'l' in "line2"
    // Note: '\r' is counted as a regular character (col increment), only '\n' triggers new line
    assert_eq!(get_line_col(text, 14), (2, 1)); // 'l' in "line3" (after \r\n)
}

#[test]
fn test_get_line_col_unicode_characters() {
    // "hello 世界\nworld": '世' starts at byte 6, '界' at byte 9, '\n' at byte 12, 'w' at byte 13
    let text = "hello 世界\nworld";
    assert_eq!(get_line_col(text, 6), (0, 6)); // '世' at byte 6: line 0, col 6
    assert_eq!(get_line_col(text, 9), (0, 7)); // '界' at byte 9: line 0, col 7
    assert_eq!(get_line_col(text, 13), (1, 0)); // 'w' at byte 13: line 1, col 0
}

#[test]
fn test_get_line_col_fast_multiline() {
    let text = "line1\nline2\nline3";
    let offsets = newline_offsets(text);
    assert_eq!(get_line_col_fast(text, 0, &offsets), (0, 0));
    assert_eq!(get_line_col_fast(text, 6, &offsets), (1, 0));
    assert_eq!(get_line_col_fast(text, 12, &offsets), (2, 0));
    assert_eq!(get_line_col_fast(text, 17, &offsets), (2, 5));
}

#[test]
fn test_get_line_col_fast_empty_text() {
    let text = "";
    let offsets = newline_offsets(text);
    assert_eq!(get_line_col_fast(text, 0, &offsets), (0, 0));
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
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[0].col, 6);
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
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[1].line, 1);
    assert_eq!(matches[2].line, 2);
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
    assert_eq!(matches[0].line, 0);
    assert_eq!(matches[1].line, 0);
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
fn test_find_matches_line_col_accuracy() {
    let text = "line1\nline2 hello\nline3";
    let matches = find_matches(text, "hello", false, false);
    assert_eq!(matches.len(), 1);
    assert_eq!(matches[0].line, 1);
    assert_eq!(matches[0].col, 6); // "hello" starts at column 6 of line 2
}

#[test]
fn test_find_matches_with_scratch_matches_baseline() {
    let text = "Alpha beta\nalpha BETA\nalpha";
    let query = "alpha";
    let baseline = find_matches(text, query, false, false);
    let mut newline_offsets_scratch = Vec::new();
    let mut lowercase_text_scratch = String::new();
    let mut lowercase_offsets_scratch = Vec::new();
    let with_scratch = find_matches_with_scratch(
        text,
        query,
        false,
        false,
        &mut newline_offsets_scratch,
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
    let mut newline_offsets_scratch = vec![999, 1000, 1001];
    let mut lowercase_text_scratch = "stale".repeat(64);
    let mut lowercase_offsets_scratch = vec![42, 43, 44];
    let first = find_matches_with_scratch(
        "line1\nline2\nline3",
        "line",
        false,
        false,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    );
    assert_eq!(first.len(), 3);

    let second = find_matches_with_scratch(
        "short",
        "sh",
        false,
        false,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].line, 0);
    assert_eq!(second[0].col, 0);
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
#[gpui::test]
fn test_search_bar_hidden_by_default(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, _content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|_window, cx| {
        assert!(!search_bar.read(cx).is_visible());
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
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
#[gpui::test]
fn test_open_search_sets_show_search_and_close_clears_it(cx: &mut TestAppContext) {
    let (fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        assert!(!search_bar.read(cx).is_visible());

        fulgur.update(cx, |this, cx| {
            this.find_in_file(window, cx);
        });
        assert!(search_bar.read(cx).is_visible());

        search_bar.update(cx, |bar, cx| {
            bar.close(Some(content.clone()), cx);
        });
        assert!(!search_bar.read(cx).is_visible());
    });
}

// ========== Default toggle state ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
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
#[gpui::test]
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
            bar.perform_search(Some(content.clone()), window, cx);
            assert_eq!(bar.search_matches.len(), 3);

            bar.match_case = true;
            bar.perform_search(Some(content.clone()), window, cx);
            assert_eq!(bar.search_matches.len(), 1);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
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
            bar.perform_search(Some(content.clone()), window, cx);
            assert_eq!(bar.search_matches.len(), 4);

            bar.match_whole_word = true;
            bar.perform_search(Some(content.clone()), window, cx);
            assert_eq!(bar.search_matches.len(), 2);
        });
    });
}

// ========== Match count state ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
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
            bar.perform_search(Some(content.clone()), window, cx);

            assert!(bar.search_matches.is_empty());
            assert!(bar.current_match_index.is_none());
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
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
            bar.perform_search(Some(content.clone()), window, cx);
            assert_eq!(bar.search_matches.len(), 3);
            assert!(bar.current_match_index.is_some());

            bar.close(Some(content.clone()), cx);

            assert!(!bar.show_search);
            assert!(bar.search_matches.is_empty());
            assert!(bar.current_match_index.is_none());
        });
    });
}

// ========== Navigation ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_gpui_search_next_previous_wrap_and_cursor(cx: &mut TestAppContext) {
    let (_fulgur, search_bar, content, mut visual_cx) = setup_search(cx);

    visual_cx.update(|window, cx| {
        content.update(cx, |content, cx| {
            content.set_value("aaa\nbbb\nccc", window, cx);
        });

        search_bar.update(cx, |bar, cx| {
            bar.search_matches = vec![
                create_match(0, 1, 0, 0),
                create_match(4, 5, 1, 0),
                create_match(8, 9, 2, 0),
            ];
            bar.current_match_index = Some(2);

            bar.search_next(Some(content.clone()), window, cx);
            assert_eq!(bar.current_match_index, Some(0));
            let cursor = content.read(cx).cursor_position();
            assert_eq!(cursor.line, 0);
            assert_eq!(cursor.character, 0);

            bar.search_previous(Some(content.clone()), window, cx);
            assert_eq!(bar.current_match_index, Some(2));
            let cursor = content.read(cx).cursor_position();
            assert_eq!(cursor.line, 2);
            assert_eq!(cursor.character, 0);
        });
    });
}

// ========== Replace ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
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

            bar.perform_search(Some(content.clone()), window, cx);
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
#[gpui::test]
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

            bar.perform_search(Some(content.clone()), window, cx);
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
#[gpui::test]
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

            bar.perform_search(Some(content.clone()), window, cx);
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
#[gpui::test]
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

            bar.perform_search(Some(content.clone()), window, cx);
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
#[gpui::test]
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

            bar.perform_search(Some(content.clone()), window, cx);
            assert_eq!(bar.search_matches.len(), 3);

            bar.replace_all(Some(content.clone()), window, cx);

            let text = content.read(cx).text().to_string();
            assert_eq!(text, "bb");
            assert!(bar.search_matches.is_empty());
            assert_eq!(bar.current_match_index, None);
        });
    });
}
