use crate::fulgur::Fulgur;
use gpui::*;
use gpui_component::input::Position;
use lsp_types::{Diagnostic, DiagnosticSeverity};

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

/// Get line and column from byte position
///
/// ### Arguments
/// - `text`: The text
/// - `pos`: The byte position
///
/// ### Returns
/// - `(usize, usize)`: A tuple of (line, column)
pub(super) fn get_line_col(text: &str, pos: usize) -> (usize, usize) {
    let mut line = 0;
    let mut col = 0;
    for (i, ch) in text.chars().enumerate() {
        if i >= pos {
            break;
        }
        if ch == '\n' {
            line += 1;
            col = 0;
        } else {
            col += 1;
        }
    }
    (line, col)
}

/// Find all matches in the text
///
/// ### Arguments
/// - `text`: The text to search in
/// - `query`: The search query
/// - `match_case`: Whether to match case
/// - `match_whole_word`: Whether to match whole words only
///
/// ### Returns
/// - `Vec<SearchMatch>`: A vector of search matches
pub(super) fn find_matches(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
) -> Vec<SearchMatch> {
    let mut matches = Vec::new();
    if query.is_empty() {
        return matches;
    }
    let search_text = if match_case {
        text.to_string()
    } else {
        text.to_lowercase()
    };
    let search_query = if match_case {
        query.to_string()
    } else {
        query.to_lowercase()
    };
    let mut start_pos = 0;
    while let Some(pos) = search_text[start_pos..].find(&search_query) {
        let absolute_pos = start_pos + pos;
        let end_pos = absolute_pos + query.len();
        if match_whole_word {
            let is_word_start = absolute_pos == 0
                || !text
                    .chars()
                    .nth(absolute_pos - 1)
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let is_word_end = end_pos >= text.len()
                || !text
                    .chars()
                    .nth(end_pos)
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');

            if !is_word_start || !is_word_end {
                start_pos = absolute_pos + 1;
                continue;
            }
        }
        let (line, col) = get_line_col(text, absolute_pos);
        matches.push(SearchMatch {
            start: absolute_pos,
            end: end_pos,
            line,
            col,
        });
        start_pos = absolute_pos + 1;
    }
    matches
}

impl Fulgur {
    /// Close the search bar and clear highlighting
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_state.show_search = false;
        if let Some(active_index) = self.active_tab_index
            && let Some(tab) = self.tabs.get(active_index)
            && let Some(editor_tab) = tab.as_editor()
        {
            editor_tab.content.update(cx, |content, _cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    diagnostics.clear();
                }
            });
        }
        self.search_state.search_matches.clear();
        self.search_state.current_match_index = None;
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Find in file
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn find_in_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_state.show_search = !self.search_state.show_search;
        if self.search_state.show_search {
            let search_focus = self.search_state.search_input.read(cx).focus_handle(cx);
            window.focus(&search_focus);
            self.perform_search(window, cx);
        } else {
            self.close_search(window, cx);
        }
        cx.notify();
    }

    /// Perform search in the active tab
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn perform_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_state.search_matches.clear();
        self.search_state.current_match_index = None;
        let query = self.search_state.search_input.read(cx).text().to_string();
        if let Some(active_index) = self.active_tab_index
            && let Some(tab) = self.tabs.get(active_index)
            && let Some(editor_tab) = tab.as_editor()
        {
            editor_tab.content.update(cx, |content, _cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    diagnostics.clear();
                }
            });
            if query.is_empty() {
                cx.notify();
                return;
            }
            let text = editor_tab.content.read(cx).text().to_string();
            let cursor_pos = editor_tab.content.read(cx).cursor();
            self.search_state.search_matches = self.find_matches(&text, &query);
            editor_tab.content.update(cx, |content, cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    for search_match in &self.search_state.search_matches {
                        let diagnostic = Diagnostic {
                            range: lsp_types::Range {
                                start: Position {
                                    line: search_match.line as u32,
                                    character: search_match.col as u32,
                                },
                                end: Position {
                                    line: search_match.line as u32,
                                    character: (search_match.col
                                        + (search_match.end - search_match.start))
                                        as u32,
                                },
                            },
                            severity: Some(DiagnosticSeverity::WARNING),
                            message: "Search match".to_string(),
                            source: None,
                            code: None,
                            related_information: None,
                            tags: None,
                            code_description: None,
                            data: None,
                        };
                        diagnostics.push(diagnostic);
                    }
                }
                cx.notify();
            });
            if !self.search_state.search_matches.is_empty() {
                let mut found_after_cursor = false;
                for (idx, m) in self.search_state.search_matches.iter().enumerate() {
                    if m.start >= cursor_pos {
                        self.search_state.current_match_index = Some(idx);
                        found_after_cursor = true;
                        break;
                    }
                }
                if !found_after_cursor {
                    self.search_state.current_match_index = Some(0);
                }
                self.highlight_current_match(window, cx);
            }
        }

        cx.notify();
    }

    /// Find all matches in the text
    ///
    /// ### Arguments
    /// - `text`: The text to search in
    /// - `query`: The search query
    ///
    /// ### Returns
    /// - `Vec<SearchMatch>`: A vector of search matches
    fn find_matches(&self, text: &str, query: &str) -> Vec<SearchMatch> {
        find_matches(
            text,
            query,
            self.search_state.match_case,
            self.search_state.match_whole_word,
        )
    }

    /// Navigate to the next search match
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn search_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.search_state.current_match_index {
            self.search_state.current_match_index =
                Some((current + 1) % self.search_state.search_matches.len());
        } else {
            self.search_state.current_match_index = Some(0);
        }
        self.highlight_current_match(window, cx);
        cx.notify();
    }

    /// Navigate to the previous search match
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn search_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.search_state.current_match_index {
            self.search_state.current_match_index = Some(if current == 0 {
                self.search_state.search_matches.len() - 1
            } else {
                current - 1
            });
        } else {
            self.search_state.current_match_index = Some(0);
        }
        self.highlight_current_match(window, cx);
        cx.notify();
    }

    /// Highlight the current search match
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    fn highlight_current_match(&self, window: &mut Window, cx: &mut App) {
        if let Some(match_index) = self.search_state.current_match_index
            && let Some(search_match) = self.search_state.search_matches.get(match_index)
            && let Some(active_index) = self.active_tab_index
            && let Some(tab) = self.tabs.get(active_index)
            && let Some(editor_tab) = tab.as_editor()
        {
            editor_tab.content.update(cx, |content, cx| {
                content.set_cursor_position(
                    Position {
                        line: search_match.line as u32,
                        character: search_match.col as u32,
                    },
                    window,
                    cx,
                );
            });
        }
    }

    /// Replace the current search match
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(match_index) = self.search_state.current_match_index
            && let Some(search_match) = self.search_state.search_matches.get(match_index).cloned()
            && let Some(active_index) = self.active_tab_index
            && let Some(tab) = self.tabs.get_mut(active_index)
            && let Some(editor_tab) = tab.as_editor_mut()
        {
            let replace_text = self.search_state.replace_input.read(cx).text().to_string();
            let text = editor_tab.content.read(cx).text().to_string();
            let mut new_text = String::new();
            new_text.push_str(&text[..search_match.start]);
            new_text.push_str(&replace_text);
            new_text.push_str(&text[search_match.end..]);
            editor_tab.content.update(cx, |content, cx| {
                content.set_value(&new_text, window, cx);
            });
            self.perform_search(window, cx);
            if !self.search_state.search_matches.is_empty() {
                if match_index < self.search_state.search_matches.len() {
                    self.search_state.current_match_index = Some(match_index);
                } else {
                    self.search_state.current_match_index = Some(0);
                }
                self.highlight_current_match(window, cx);
            }
        }
        cx.notify();
    }

    /// Replace all search matches
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.search_matches.is_empty() {
            return;
        }
        if let Some(active_index) = self.active_tab_index {
            let replace_text = self.search_state.replace_input.read(cx).text().to_string();
            let search_query = self.search_state.search_input.read(cx).text().to_string();
            let match_case = self.search_state.match_case;
            let match_whole_word = self.search_state.match_whole_word;
            if let Some(tab) = self.tabs.get(active_index)
                && let Some(editor_tab) = tab.as_editor()
            {
                let text = editor_tab.content.read(cx).text().to_string();
                let new_text = if match_case {
                    if match_whole_word {
                        replace_whole_words(&self.search_state.search_matches, &text, &replace_text)
                    } else {
                        text.replace(&search_query, &replace_text)
                    }
                } else if match_whole_word {
                    replace_whole_words_case_insensitive(
                        &self.search_state.search_matches,
                        &text,
                        &replace_text,
                    )
                } else {
                    replace_case_insensitive(
                        &self.search_state.search_matches,
                        &text,
                        &replace_text,
                    )
                };
                if let Some(tab) = self.tabs.get_mut(active_index)
                    && let Some(editor_tab_mut) = tab.as_editor_mut()
                {
                    editor_tab_mut.content.update(cx, |content, cx| {
                        content.set_value(&new_text, window, cx);
                    });
                }
                self.search_state.search_matches.clear();
                self.search_state.current_match_index = None;
            }
        }
        cx.notify();
    }
}

/// Replace all occurrences case-insensitively
///
/// ### Arguments
/// - `search_matches`: The search matches
/// - `text`: The text to search in
/// - `replace`: The replacement text
///
/// ### Returns
/// - `String`: The text with replacements
fn replace_case_insensitive(search_matches: &[SearchMatch], text: &str, replace: &str) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    for m in search_matches.iter() {
        result.push_str(&text[last_pos..m.start]);
        result.push_str(replace);
        last_pos = m.end;
    }
    result.push_str(&text[last_pos..]);
    result
}

/// Replace whole words only
///
/// ### Arguments
/// - `search_matches`: The search matches
/// - `text`: The text to search in
/// - `replace`: The replacement text
///
/// ### Returns
/// - `String`: The text with replacements
fn replace_whole_words(search_matches: &[SearchMatch], text: &str, replace: &str) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    for m in search_matches.iter() {
        result.push_str(&text[last_pos..m.start]);
        result.push_str(replace);
        last_pos = m.end;
    }
    result.push_str(&text[last_pos..]);
    result
}

/// Replace whole words case-insensitively
///
/// ### Arguments
/// - `search_matches`: The search matches
/// - `text`: The text to search in
/// - `replace`: The replacement text
///
/// ### Returns
/// - `String`: The text with replacements
fn replace_whole_words_case_insensitive(
    search_matches: &[SearchMatch],
    text: &str,
    replace: &str,
) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    for m in search_matches.iter() {
        result.push_str(&text[last_pos..m.start]);
        result.push_str(replace);
        last_pos = m.end;
    }
    result.push_str(&text[last_pos..]);
    result
}

#[cfg(test)]
mod tests {
    use super::{
        SearchMatch, find_matches, get_line_col, replace_case_insensitive, replace_whole_words,
        replace_whole_words_case_insensitive,
    };

    fn create_match(start: usize, end: usize, line: usize, col: usize) -> SearchMatch {
        SearchMatch {
            start,
            end,
            line,
            col,
        }
    }

    #[test]
    fn test_replace_case_insensitive_single_match() {
        let text = "Hello World";
        let matches = vec![create_match(0, 5, 0, 0)]; // "Hello"
        let result = replace_case_insensitive(&matches, text, "Hi");
        assert_eq!(result, "Hi World");
    }

    #[test]
    fn test_replace_case_insensitive_multiple_matches() {
        let text = "hello hello hello";
        let matches = vec![
            create_match(0, 5, 0, 0),    // "hello"
            create_match(6, 11, 0, 6),   // "hello"
            create_match(12, 17, 0, 12), // "hello"
        ];
        let result = replace_case_insensitive(&matches, text, "hi");
        assert_eq!(result, "hi hi hi");
    }

    #[test]
    fn test_replace_case_insensitive_no_matches() {
        let text = "Hello World";
        let matches = vec![];
        let result = replace_case_insensitive(&matches, text, "Hi");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_replace_case_insensitive_match_at_start() {
        let text = "test string";
        let matches = vec![create_match(0, 4, 0, 0)]; // "test"
        let result = replace_case_insensitive(&matches, text, "example");
        assert_eq!(result, "example string");
    }

    #[test]
    fn test_replace_case_insensitive_match_at_end() {
        let text = "test string";
        let matches = vec![create_match(5, 11, 0, 5)]; // "string"
        let result = replace_case_insensitive(&matches, text, "text");
        assert_eq!(result, "test text");
    }

    #[test]
    fn test_replace_case_insensitive_multiline() {
        let text = "line1\nline2\nline3";
        let matches = vec![
            create_match(0, 5, 0, 0),   // "line1"
            create_match(6, 11, 1, 0),  // "line2"
            create_match(12, 17, 2, 0), // "line3"
        ];
        let result = replace_case_insensitive(&matches, text, "replaced");
        assert_eq!(result, "replaced\nreplaced\nreplaced");
    }

    #[test]
    fn test_replace_case_insensitive_empty_replace() {
        let text = "hello world";
        let matches = vec![create_match(0, 5, 0, 0)]; // "hello"
        let result = replace_case_insensitive(&matches, text, "");
        assert_eq!(result, " world");
    }

    #[test]
    fn test_replace_whole_words_single_match() {
        let text = "Hello World";
        let matches = vec![create_match(0, 5, 0, 0)]; // "Hello"
        let result = replace_whole_words(&matches, text, "Hi");
        assert_eq!(result, "Hi World");
    }

    #[test]
    fn test_replace_whole_words_multiple_matches() {
        let text = "test test test";
        let matches = vec![
            create_match(0, 4, 0, 0),    // "test"
            create_match(5, 9, 0, 5),    // "test"
            create_match(10, 14, 0, 10), // "test"
        ];
        let result = replace_whole_words(&matches, text, "example");
        assert_eq!(result, "example example example");
    }

    #[test]
    fn test_replace_whole_words_no_matches() {
        let text = "Hello World";
        let matches = vec![];
        let result = replace_whole_words(&matches, text, "Hi");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_replace_whole_words_match_at_start() {
        let text = "word other";
        let matches = vec![create_match(0, 4, 0, 0)]; // "word"
        let result = replace_whole_words(&matches, text, "term");
        assert_eq!(result, "term other");
    }

    #[test]
    fn test_replace_whole_words_match_at_end() {
        let text = "other word";
        let matches = vec![create_match(6, 10, 0, 6)]; // "word"
        let result = replace_whole_words(&matches, text, "term");
        assert_eq!(result, "other term");
    }

    #[test]
    fn test_replace_whole_words_multiline() {
        let text = "word1\nword2\nword3";
        let matches = vec![
            create_match(0, 5, 0, 0),   // "word1"
            create_match(6, 11, 1, 0),  // "word2"
            create_match(12, 17, 2, 0), // "word3"
        ];
        let result = replace_whole_words(&matches, text, "replaced");
        assert_eq!(result, "replaced\nreplaced\nreplaced");
    }

    #[test]
    fn test_replace_whole_words_empty_replace() {
        let text = "hello world";
        let matches = vec![create_match(0, 5, 0, 0)]; // "hello"
        let result = replace_whole_words(&matches, text, "");
        assert_eq!(result, " world");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_single_match() {
        let text = "Hello World";
        let matches = vec![create_match(0, 5, 0, 0)]; // "Hello"
        let result = replace_whole_words_case_insensitive(&matches, text, "Hi");
        assert_eq!(result, "Hi World");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_multiple_matches() {
        let text = "test TEST test";
        let matches = vec![
            create_match(0, 4, 0, 0),    // "test"
            create_match(5, 9, 0, 5),    // "TEST"
            create_match(10, 14, 0, 10), // "test"
        ];
        let result = replace_whole_words_case_insensitive(&matches, text, "example");
        assert_eq!(result, "example example example");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_no_matches() {
        let text = "Hello World";
        let matches = vec![];
        let result = replace_whole_words_case_insensitive(&matches, text, "Hi");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_match_at_start() {
        let text = "word other";
        let matches = vec![create_match(0, 4, 0, 0)]; // "word"
        let result = replace_whole_words_case_insensitive(&matches, text, "term");
        assert_eq!(result, "term other");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_match_at_end() {
        let text = "other word";
        let matches = vec![create_match(6, 10, 0, 6)]; // "word"
        let result = replace_whole_words_case_insensitive(&matches, text, "term");
        assert_eq!(result, "other term");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_multiline() {
        let text = "word1\nWORD2\nword3";
        let matches = vec![
            create_match(0, 5, 0, 0),   // "word1"
            create_match(6, 11, 1, 0),  // "WORD2"
            create_match(12, 17, 2, 0), // "word3"
        ];
        let result = replace_whole_words_case_insensitive(&matches, text, "replaced");
        assert_eq!(result, "replaced\nreplaced\nreplaced");
    }

    #[test]
    fn test_replace_whole_words_case_insensitive_empty_replace() {
        let text = "hello world";
        let matches = vec![create_match(0, 5, 0, 0)]; // "hello"
        let result = replace_whole_words_case_insensitive(&matches, text, "");
        assert_eq!(result, " world");
    }

    #[test]
    fn test_replace_case_insensitive_non_sequential_matches() {
        let text = "hello world hello";
        let matches = vec![
            create_match(0, 5, 0, 0),    // "hello"
            create_match(12, 17, 0, 12), // "hello"
        ];
        let result = replace_case_insensitive(&matches, text, "hi");
        assert_eq!(result, "hi world hi");
    }

    #[test]
    fn test_replace_whole_words_non_sequential_matches() {
        let text = "test word test";
        let matches = vec![
            create_match(0, 4, 0, 0),    // "test"
            create_match(10, 14, 0, 10), // "test"
        ];
        let result = replace_whole_words(&matches, text, "example");
        assert_eq!(result, "example word example");
    }

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
        let text = "hello 世界\nworld";
        assert_eq!(get_line_col(text, 6), (0, 6)); // '世'
        assert_eq!(get_line_col(text, 9), (1, 0)); // 'w' in "world"
    }

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
}
