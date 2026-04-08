use super::SearchMatch;
use super::actions::{
    apply_replacements, find_matches, find_matches_with_scratch, get_line_col_fast,
};
use core::prelude::v1::test;

#[cfg(feature = "gpui-test-support")]
use crate::fulgur::{
    Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
};
#[cfg(feature = "gpui-test-support")]
use gpui::{
    AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    div,
};
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
        cx.set_global(SharedAppState::new(settings, pending_files));
        cx.set_global(WindowManager::new());
    });

    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
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
    // "hello 世界\nworld" — '世' starts at byte 6, '界' at byte 9, '\n' at byte 12, 'w' at byte 13
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
    let with_scratch = find_matches_with_scratch(
        text,
        query,
        false,
        false,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
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
    let first = find_matches_with_scratch(
        "line1\nline2\nline3",
        "line",
        false,
        false,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
    );
    assert_eq!(first.len(), 3);

    let second = find_matches_with_scratch(
        "short",
        "sh",
        false,
        false,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
    );
    assert_eq!(second.len(), 1);
    assert_eq!(second[0].line, 0);
    assert_eq!(second[0].col, 0);
}

// ========== Visibility control ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_render_search_bar_hidden_by_default(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            assert!(!this.search_state.show_search);
            assert!(this.render_search_bar(cx).is_none());
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_render_search_bar_visible_when_open(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.find_in_file(window, cx);
            assert!(this.search_state.show_search);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_open_search_sets_show_search_and_close_clears_it(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            assert!(!this.search_state.show_search);

            this.find_in_file(window, cx);
            assert!(this.search_state.show_search);

            this.close_search(window, cx);
            assert!(!this.search_state.show_search);
        });
    });
}

// ========== Default toggle state ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_search_toggle_defaults(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            assert!(!this.search_state.show_search);
            assert!(!this.search_state.match_case);
            assert!(!this.search_state.match_whole_word);
            assert!(this.search_state.search_matches.is_empty());
            assert!(this.search_state.current_match_index.is_none());
        });
    });
}

// ========== Toggle state reflected in search results ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_match_case_toggle_filters_results(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("Hello hello HELLO", window, cx);
            });
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("hello", window, cx);
            });

            this.search_state.match_case = false;
            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 3);

            this.search_state.match_case = true;
            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 1);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_match_whole_word_toggle_filters_results(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("test testing tested test", window, cx);
            });
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("test", window, cx);
            });

            this.search_state.match_whole_word = false;
            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 4);

            this.search_state.match_whole_word = true;
            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 2);
        });
    });
}

// ========== Match count state ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_no_match_state_when_query_not_found(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("aaa bbb ccc", window, cx);
            });
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("zzz", window, cx);
            });
            this.perform_search(window, cx);

            assert!(this.search_state.search_matches.is_empty());
            assert!(this.search_state.current_match_index.is_none());
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_close_search_clears_match_state(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("foo foo foo", window, cx);
            });
            this.search_state.show_search = true;
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 3);
            assert!(this.search_state.current_match_index.is_some());

            this.close_search(window, cx);

            assert!(!this.search_state.show_search);
            assert!(this.search_state.search_matches.is_empty());
            assert!(this.search_state.current_match_index.is_none());
        });
    });
}

// ========== Navigation ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_gpui_search_next_previous_wrap_and_cursor(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("aaa\nbbb\nccc", window, cx);
            });

            this.search_state.search_matches = vec![
                create_match(0, 1, 0, 0),
                create_match(4, 5, 1, 0),
                create_match(8, 9, 2, 0),
            ];
            this.search_state.current_match_index = Some(2);

            this.search_next(window, cx);
            assert_eq!(this.search_state.current_match_index, Some(0));
            let cursor = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .cursor_position();
            assert_eq!(cursor.line, 0);
            assert_eq!(cursor.character, 0);

            this.search_previous(window, cx);
            assert_eq!(this.search_state.current_match_index, Some(2));
            let cursor = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .cursor_position();
            assert_eq!(cursor.line, 2);
            assert_eq!(cursor.character, 0);
        });
    });
}

// ========== Replace ==========

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_gpui_replace_current_updates_text_and_matches(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("foo bar foo", window, cx);
            });

            this.search_state.match_case = false;
            this.search_state.match_whole_word = false;
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("foo", window, cx);
            });
            this.search_state.replace_input.update(cx, |input, cx| {
                input.set_value("baz", window, cx);
            });

            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 2);
            assert_eq!(this.search_state.current_match_index, Some(0));

            this.replace_current(window, cx);

            let text = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "baz bar foo");
            assert_eq!(this.search_state.search_matches.len(), 1);
            assert_eq!(this.search_state.current_match_index, Some(0));

            let cursor = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .cursor_position();
            assert_eq!(cursor.line, 0);
            assert_eq!(cursor.character, 8);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_gpui_replace_all_whole_word_only(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("test testing test", window, cx);
            });

            this.search_state.match_case = false;
            this.search_state.match_whole_word = true;
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("test", window, cx);
            });
            this.search_state.replace_input.update(cx, |input, cx| {
                input.set_value("done", window, cx);
            });

            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 2);

            this.replace_all(window, cx);

            let text = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "done testing done");
            assert!(this.search_state.search_matches.is_empty());
            assert_eq!(this.search_state.current_match_index, None);
        });
    });
}

#[cfg(feature = "gpui-test-support")]
#[gpui::test]
fn test_gpui_replace_all_case_sensitive_non_whole_word(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);

    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("aaaa", window, cx);
            });

            this.search_state.match_case = true;
            this.search_state.match_whole_word = false;
            this.search_state.search_input.update(cx, |input, cx| {
                input.set_value("aa", window, cx);
            });
            this.search_state.replace_input.update(cx, |input, cx| {
                input.set_value("b", window, cx);
            });

            this.perform_search(window, cx);
            assert_eq!(this.search_state.search_matches.len(), 3);

            this.replace_all(window, cx);

            let text = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "bb");
            assert!(this.search_state.search_matches.is_empty());
            assert_eq!(this.search_state.current_match_index, None);
        });
    });
}
