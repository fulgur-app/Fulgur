use crate::lightspeed::Lightspeed;
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

impl Lightspeed {
    /// Close the search bar and clear highlighting
    /// @param window: The window context
    /// @param cx: The application context
    pub(super) fn close_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.show_search = false;

        // Clear search highlighting from active tab
        if let Some(active_index) = self.active_tab_index {
            if let Some(tab) = self.tabs.get(active_index) {
                tab.content.update(cx, |content, _cx| {
                    if let Some(diagnostics) = content.diagnostics_mut() {
                        diagnostics.clear();
                    }
                });
            }
        }

        // Clear search results
        self.search_matches.clear();
        self.current_match_index = None;

        // Focus back on the editor
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    /// Perform search in the active tab
    /// @param window: The window context
    /// @param cx: The application context
    pub(super) fn perform_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_matches.clear();
        self.current_match_index = None;

        // Get the search query
        let query = self.search_input.read(cx).text().to_string();

        // Get the active tab content
        if let Some(active_index) = self.active_tab_index {
            if let Some(tab) = self.tabs.get(active_index) {
                // Clear existing search highlights
                tab.content.update(cx, |content, _cx| {
                    if let Some(diagnostics) = content.diagnostics_mut() {
                        diagnostics.clear();
                    }
                });

                if query.is_empty() {
                    cx.notify();
                    return;
                }

                let text = tab.content.read(cx).text().to_string();
                let cursor_pos = tab.content.read(cx).cursor();

                // Find all matches
                self.search_matches = self.find_matches(&text, &query);

                // Add visual highlighting using diagnostics (yellow background)
                tab.content.update(cx, |content, cx| {
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

                // Find the first match after the cursor, or wrap to the first match
                if !self.search_matches.is_empty() {
                    let mut found_after_cursor = false;
                    for (idx, m) in self.search_matches.iter().enumerate() {
                        if m.start >= cursor_pos {
                            self.current_match_index = Some(idx);
                            found_after_cursor = true;
                            break;
                        }
                    }

                    // If no match after cursor, wrap to first match
                    if !found_after_cursor {
                        self.current_match_index = Some(0);
                    }

                    // Jump to the match and select it
                    self.highlight_current_match(window, cx);
                }
            }
        }

        cx.notify();
    }

    /// Find all matches in the text
    /// @param text: The text to search in
    /// @param query: The search query
    /// @return: A vector of search matches
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

            // Check whole word matching if enabled
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

            // Calculate line and column
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

    /// Get line and column from byte position
    /// @param text: The text
    /// @param pos: The byte position
    /// @return: A tuple of (line, column)
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

    /// Navigate to the next search match
    /// @param window: The window context
    /// @param cx: The application context
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

    /// Navigate to the previous search match
    /// @param window: The window context
    /// @param cx: The application context
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

    /// Highlight the current search match
    /// @param window: The window context
    /// @param cx: The application context
    fn highlight_current_match(&self, window: &mut Window, cx: &mut App) {
        if let Some(match_index) = self.current_match_index {
            if let Some(search_match) = self.search_matches.get(match_index) {
                if let Some(active_index) = self.active_tab_index {
                    if let Some(tab) = self.tabs.get(active_index) {
                        // Set the cursor position to the match
                        tab.content.update(cx, |content, cx| {
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

    /// Replace the current search match
    /// @param window: The window context
    /// @param cx: The application context
    pub(super) fn replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(match_index) = self.current_match_index {
            if let Some(search_match) = self.search_matches.get(match_index).cloned() {
                if let Some(active_index) = self.active_tab_index {
                    if let Some(tab) = self.tabs.get_mut(active_index) {
                        let replace_text = self.replace_input.read(cx).text().to_string();

                        // Get current text
                        let text = tab.content.read(cx).text().to_string();

                        // Replace the match in the text
                        let mut new_text = String::new();
                        new_text.push_str(&text[..search_match.start]);
                        new_text.push_str(&replace_text);
                        new_text.push_str(&text[search_match.end..]);

                        // Update the content
                        tab.content.update(cx, |content, cx| {
                            content.set_value(&new_text, window, cx);
                        });

                        // Re-run search to update matches
                        self.perform_search(window, cx);

                        // If there are still matches, move to the current or next one
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
        cx.notify();
    }

    /// Replace all search matches
    /// @param window: The window context
    /// @param cx: The application context
    pub(super) fn replace_all(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_matches.is_empty() {
            return;
        }

        if let Some(active_index) = self.active_tab_index {
            let replace_text = self.replace_input.read(cx).text().to_string();
            let search_query = self.search_input.read(cx).text().to_string();
            let match_case = self.match_case;
            let match_whole_word = self.match_whole_word;

            // Get the current text
            if let Some(tab) = self.tabs.get(active_index) {
                let text = tab.content.read(cx).text().to_string();

                // Perform replacement
                let new_text = if match_case {
                    if match_whole_word {
                        self.replace_whole_words(&text, &replace_text)
                    } else {
                        text.replace(&search_query, &replace_text)
                    }
                } else {
                    if match_whole_word {
                        self.replace_whole_words_case_insensitive(&text, &replace_text)
                    } else {
                        self.replace_case_insensitive(&text, &replace_text)
                    }
                };

                // Update the content
                if let Some(tab) = self.tabs.get_mut(active_index) {
                    tab.content.update(cx, |content, cx| {
                        content.set_value(&new_text, window, cx);
                    });
                }

                // Clear search matches
                self.search_matches.clear();
                self.current_match_index = None;
            }
        }
        cx.notify();
    }

    /// Replace all occurrences case-insensitively
    /// @param text: The text to search in
    /// @param replace: The replacement text
    /// @return: The text with replacements
    fn replace_case_insensitive(&self, text: &str, replace: &str) -> String {
        let mut result = String::new();
        let mut last_pos = 0;

        for m in self.search_matches.iter() {
            result.push_str(&text[last_pos..m.start]);
            result.push_str(replace);
            last_pos = m.end;
        }
        result.push_str(&text[last_pos..]);
        result
    }

    /// Replace whole words only
    /// @param text: The text to search in
    /// @param replace: The replacement text
    /// @return: The text with replacements
    fn replace_whole_words(&self, text: &str, replace: &str) -> String {
        let mut result = String::new();
        let mut last_pos = 0;

        for m in self.search_matches.iter() {
            result.push_str(&text[last_pos..m.start]);
            result.push_str(replace);
            last_pos = m.end;
        }
        result.push_str(&text[last_pos..]);
        result
    }

    /// Replace whole words case-insensitively
    /// @param text: The text to search in
    /// @param replace: The replacement text
    /// @return: The text with replacements
    fn replace_whole_words_case_insensitive(&self, text: &str, replace: &str) -> String {
        let mut result = String::new();
        let mut last_pos = 0;

        for m in self.search_matches.iter() {
            result.push_str(&text[last_pos..m.start]);
            result.push_str(replace);
            last_pos = m.end;
        }
        result.push_str(&text[last_pos..]);
        result
    }
}
