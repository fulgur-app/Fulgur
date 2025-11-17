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

impl Fulgur {
    // Close the search bar and clear highlighting
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_search = false;
        if let Some(active_index) = self.active_tab_index {
            if let Some(tab) = self.tabs.get(active_index) {
                if let Some(editor_tab) = tab.as_editor() {
                    editor_tab.content.update(cx, |content, _cx| {
                        if let Some(diagnostics) = content.diagnostics_mut() {
                            diagnostics.clear();
                        }
                    });
                }
            }
        }
        self.search_matches.clear();
        self.current_match_index = None;
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    // Find in file
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn find_in_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_search = !self.show_search;
        if self.show_search {
            let search_focus = self.search_input.read(cx).focus_handle(cx);
            window.focus(&search_focus);
            self.perform_search(window, cx);
        } else {
            self.close_search(window, cx);
        }
        cx.notify();
    }

    // Perform search in the active tab
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn perform_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_matches.clear();
        self.current_match_index = None;
        let query = self.search_input.read(cx).text().to_string();
        if let Some(active_index) = self.active_tab_index {
            if let Some(tab) = self.tabs.get(active_index) {
                if let Some(editor_tab) = tab.as_editor() {
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
                    self.search_matches = self.find_matches(&text, &query);
                    editor_tab.content.update(cx, |content, cx| {
                        if let Some(diagnostics) = content.diagnostics_mut() {
                            for search_match in &self.search_matches {
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
                    if !self.search_matches.is_empty() {
                        let mut found_after_cursor = false;
                        for (idx, m) in self.search_matches.iter().enumerate() {
                            if m.start >= cursor_pos {
                                self.current_match_index = Some(idx);
                                found_after_cursor = true;
                                break;
                            }
                        }
                        if !found_after_cursor {
                            self.current_match_index = Some(0);
                        }
                        self.highlight_current_match(window, cx);
                    }
                }
            }
        }
        cx.notify();
    }

    // Find all matches in the text
    // @param text: The text to search in
    // @param query: The search query
    // @return: A vector of search matches
    fn find_matches(&self, text: &str, query: &str) -> Vec<SearchMatch> {
        let mut matches = Vec::new();
        if query.is_empty() {
            return matches;
        }
        let search_text = if self.match_case {
            text.to_string()
        } else {
            text.to_lowercase()
        };
        let search_query = if self.match_case {
            query.to_string()
        } else {
            query.to_lowercase()
        };
        let mut start_pos = 0;
        while let Some(pos) = search_text[start_pos..].find(&search_query) {
            let absolute_pos = start_pos + pos;
            let end_pos = absolute_pos + query.len();
            if self.match_whole_word {
                let is_word_start = absolute_pos == 0
                    || !text
                        .chars()
                        .nth(absolute_pos - 1)
                        .map_or(false, |c| c.is_alphanumeric() || c == '_');
                let is_word_end = end_pos >= text.len()
                    || !text
                        .chars()
                        .nth(end_pos)
                        .map_or(false, |c| c.is_alphanumeric() || c == '_');

                if !is_word_start || !is_word_end {
                    start_pos = absolute_pos + 1;
                    continue;
                }
            }
            let (line, col) = self.get_line_col(text, absolute_pos);
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

    // Get line and column from byte position
    // @param text: The text
    // @param pos: The byte position
    // @return: A tuple of (line, column)
    fn get_line_col(&self, text: &str, pos: usize) -> (usize, usize) {
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

    // Navigate to the next search match
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn search_next(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.current_match_index {
            self.current_match_index = Some((current + 1) % self.search_matches.len());
        } else {
            self.current_match_index = Some(0);
        }
        self.highlight_current_match(window, cx);
        cx.notify();
    }

    // Navigate to the previous search match
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn search_previous(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.current_match_index {
            self.current_match_index = Some(if current == 0 {
                self.search_matches.len() - 1
            } else {
                current - 1
            });
        } else {
            self.current_match_index = Some(0);
        }
        self.highlight_current_match(window, cx);
        cx.notify();
    }

    // Highlight the current search match
    // @param window: The window context
    // @param cx: The application context
    fn highlight_current_match(&self, window: &mut Window, cx: &mut App) {
        if let Some(match_index) = self.current_match_index {
            if let Some(search_match) = self.search_matches.get(match_index) {
                if let Some(active_index) = self.active_tab_index {
                    if let Some(tab) = self.tabs.get(active_index) {
                        if let Some(editor_tab) = tab.as_editor() {
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
                }
            }
        }
    }

    // Replace the current search match
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(match_index) = self.current_match_index {
            if let Some(search_match) = self.search_matches.get(match_index).cloned() {
                if let Some(active_index) = self.active_tab_index {
                    if let Some(tab) = self.tabs.get_mut(active_index) {
                        if let Some(editor_tab) = tab.as_editor_mut() {
                            let replace_text = self.replace_input.read(cx).text().to_string();
                            let text = editor_tab.content.read(cx).text().to_string();
                            let mut new_text = String::new();
                            new_text.push_str(&text[..search_match.start]);
                            new_text.push_str(&replace_text);
                            new_text.push_str(&text[search_match.end..]);
                            editor_tab.content.update(cx, |content, cx| {
                                content.set_value(&new_text, window, cx);
                            });
                            self.perform_search(window, cx);
                            if !self.search_matches.is_empty() {
                                if match_index < self.search_matches.len() {
                                    self.current_match_index = Some(match_index);
                                } else {
                                    self.current_match_index = Some(0);
                                }
                                self.highlight_current_match(window, cx);
                            }
                        }
                    }
                }
            }
        }
        cx.notify();
    }

    // Replace all search matches
    // @param window: The window context
    // @param cx: The application context
    pub(super) fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(active_index) = self.active_tab_index {
            let replace_text = self.replace_input.read(cx).text().to_string();
            let search_query = self.search_input.read(cx).text().to_string();
            let match_case = self.match_case;
            let match_whole_word = self.match_whole_word;
            if let Some(tab) = self.tabs.get(active_index) {
                if let Some(editor_tab) = tab.as_editor() {
                    let text = editor_tab.content.read(cx).text().to_string();
                    let new_text = if match_case {
                        if match_whole_word {
                            replace_whole_words(&self.search_matches, &text, &replace_text)
                        } else {
                            text.replace(&search_query, &replace_text)
                        }
                    } else {
                        if match_whole_word {
                            replace_whole_words_case_insensitive(
                                &self.search_matches,
                                &text,
                                &replace_text,
                            )
                        } else {
                            replace_case_insensitive(&self.search_matches, &text, &replace_text)
                        }
                    };
                    if let Some(tab) = self.tabs.get_mut(active_index) {
                        if let Some(editor_tab_mut) = tab.as_editor_mut() {
                            editor_tab_mut.content.update(cx, |content, cx| {
                                content.set_value(&new_text, window, cx);
                            });
                        }
                    }
                    self.search_matches.clear();
                    self.current_match_index = None;
                }
            }
        }
        cx.notify();
    }
}

// Replace all occurrences case-insensitively
// @param search_matches: The search matches
// @param text: The text to search in
// @param replace: The replacement text
// @return: The text with replacements
fn replace_case_insensitive(
    search_matches: &Vec<SearchMatch>,
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

// Replace whole words only
// @param search_matches: The search matches
// @param text: The text to search in
// @param replace: The replacement text
// @return: The text with replacements
fn replace_whole_words(search_matches: &Vec<SearchMatch>, text: &str, replace: &str) -> String {
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

// Replace whole words case-insensitively
// @param search_matches: The search matches
// @param text: The text to search in
// @param replace: The replacement text
// @return: The text with replacements
fn replace_whole_words_case_insensitive(
    search_matches: &Vec<SearchMatch>,
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
        SearchMatch, replace_case_insensitive, replace_whole_words,
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
}
