use crate::fulgur::Fulgur;
use gpui::{App, Context, Focusable, Window};
use gpui_component::input::Position;
use lsp_types::{Diagnostic, DiagnosticSeverity};
use std::borrow::Cow;

use super::SearchMatch;

/// Refresh the newline-offset scratch buffer for fast line/column lookup.
///
/// ### Arguments
/// - `text`: Source text being searched
/// - `newline_offsets_scratch`: Reusable scratch vector populated with `\n` byte offsets
fn refresh_newline_offsets(text: &str, newline_offsets_scratch: &mut Vec<usize>) {
    newline_offsets_scratch.clear();
    newline_offsets_scratch.extend(
        text.bytes()
            .enumerate()
            .filter_map(|(i, b)| if b == b'\n' { Some(i) } else { None }),
    );
}

/// Rebuild the lowercased search text together with a byte-offset map back to the original text.
///
/// ### Arguments
/// - `text`: The original source text
/// - `lowercase_text_scratch`: Reusable buffer filled with the lowercased text
/// - `lowercase_offsets_scratch`: Reusable buffer filled with the lowercased-to-original offset map
fn rebuild_lowercase_text(
    text: &str,
    lowercase_text_scratch: &mut String,
    lowercase_offsets_scratch: &mut Vec<usize>,
) {
    lowercase_text_scratch.clear();
    lowercase_offsets_scratch.clear();
    for (orig_offset, ch) in text.char_indices() {
        let before = lowercase_text_scratch.len();
        for lowered in ch.to_lowercase() {
            lowercase_text_scratch.push(lowered);
        }
        let added = lowercase_text_scratch.len() - before;
        for _ in 0..added {
            lowercase_offsets_scratch.push(orig_offset);
        }
    }
    lowercase_offsets_scratch.push(text.len());
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
#[cfg(test)]
pub(super) fn find_matches(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
) -> Vec<SearchMatch> {
    let mut newline_offsets_scratch = Vec::new();
    let mut lowercase_text_scratch = String::new();
    let mut lowercase_offsets_scratch = Vec::new();
    find_matches_with_scratch(
        text,
        query,
        match_case,
        match_whole_word,
        &mut newline_offsets_scratch,
        &mut lowercase_text_scratch,
        &mut lowercase_offsets_scratch,
    )
}

/// Find all matches in the text while reusing caller-provided scratch buffers.
///
/// ### Arguments
/// - `text`: The text to search in
/// - `query`: The search query
/// - `match_case`: Whether to match case
/// - `match_whole_word`: Whether to match whole words only
/// - `newline_offsets_scratch`: Reusable newline-offset buffer
/// - `lowercase_text_scratch`: Reusable lowercase-text buffer
/// - `lowercase_offsets_scratch`: Reusable lowercased-to-original byte-offset map
///
/// ### Returns
/// - `Vec<SearchMatch>`: A vector of search matches with offsets into the original `text`
pub(super) fn find_matches_with_scratch(
    text: &str,
    query: &str,
    match_case: bool,
    match_whole_word: bool,
    newline_offsets_scratch: &mut Vec<usize>,
    lowercase_text_scratch: &mut String,
    lowercase_offsets_scratch: &mut Vec<usize>,
) -> Vec<SearchMatch> {
    let mut matches = Vec::new();
    if query.is_empty() {
        return matches;
    }

    refresh_newline_offsets(text, newline_offsets_scratch);

    // For case-insensitive search the haystack is a lowercased copy whose byte layout
    // can differ from the original. Offsets found in that copy are translated back to
    // the original text through `lowercase_offsets_scratch`.
    let search_text = if match_case {
        text
    } else {
        rebuild_lowercase_text(text, lowercase_text_scratch, lowercase_offsets_scratch);
        lowercase_text_scratch.as_str()
    };
    let search_query: Cow<str> = if match_case {
        Cow::Borrowed(query)
    } else {
        Cow::Owned(query.to_lowercase())
    };

    let mut start_pos = 0;
    while let Some(pos) = search_text[start_pos..].find(search_query.as_ref()) {
        let search_start = start_pos + pos;
        let search_end = search_start + search_query.len();
        // Map both endpoints from search-text space back to original-text space.
        // With case-sensitive search the two spaces are identical.
        let (match_start, match_end) = if match_case {
            (search_start, search_end)
        } else {
            (
                lowercase_offsets_scratch[search_start],
                lowercase_offsets_scratch[search_end],
            )
        };
        if match_whole_word {
            let is_word_start = match_start == 0
                || !text[..match_start]
                    .chars()
                    .next_back()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');
            let is_word_end = match_end >= text.len()
                || !text[match_end..]
                    .chars()
                    .next()
                    .is_some_and(|c| c.is_alphanumeric() || c == '_');

            if !is_word_start || !is_word_end {
                start_pos = advance_past_char(search_text, search_start);
                continue;
            }
        }
        let (line, col) = get_line_col_fast(text, match_start, newline_offsets_scratch);
        matches.push(SearchMatch {
            start: match_start,
            end: match_end,
            line,
            col,
        });
        start_pos = advance_past_char(search_text, search_start);
    }
    matches
}

/// Advance a scan cursor past the character starting at `pos`, staying on a char boundary.
///
/// ### Arguments
/// - `text`: The text being scanned
/// - `pos`: A char-boundary byte offset within `text`
///
/// ### Returns
/// - `usize`: The byte offset of the next character boundary after `pos`
fn advance_past_char(text: &str, pos: usize) -> usize {
    let char_len = text[pos..].chars().next().map_or(1, char::len_utf8);
    pos + char_len
}

/// Get line and column from byte position using precomputed newline offsets
///  
/// ### Arguments
/// - `text`: The text
/// - `byte_pos`: The byte position
/// - `newline_offsets`: Precomputed byte offsets of all newline characters
///
/// ### Returns
/// - `(usize, usize)`: A tuple of (line, column)
pub(super) fn get_line_col_fast(
    text: &str,
    byte_pos: usize,
    newline_offsets: &[usize],
) -> (usize, usize) {
    let pos = byte_pos.min(text.len());
    let line = newline_offsets.partition_point(|&nl| nl < pos);
    let line_start = if line == 0 {
        0
    } else {
        newline_offsets[line - 1] + 1
    };
    let col = text[line_start..pos].chars().count();
    (line, col)
}

/// Replace text at all match positions with the replacement string
///
/// ### Arguments
/// - `search_matches`: The precomputed search match positions
/// - `text`: The original text
/// - `replace`: The replacement text
///
/// ### Returns
/// - `String`: The text with all matches replaced
pub(super) fn apply_replacements(
    search_matches: &[SearchMatch],
    text: &str,
    replace: &str,
) -> String {
    let mut result = String::new();
    let mut last_pos = 0;
    for m in search_matches {
        // Defensive guard against stale offsets: skip any match that does not
        // align with the current text's char boundaries, runs past its end, or
        // overlaps the previous replacement. This prevents an out-of-bounds or
        // non-char-boundary slice from panicking if the buffer changed.
        if m.start < last_pos
            || m.end > text.len()
            || m.start > m.end
            || !text.is_char_boundary(m.start)
            || !text.is_char_boundary(m.end)
        {
            continue;
        }
        result.push_str(&text[last_pos..m.start]);
        result.push_str(replace);
        last_pos = m.end;
    }
    result.push_str(&text[last_pos..]);
    result
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
            window.focus(&search_focus, cx);
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
        let query = self.search_state.search_input.read(cx).text().to_string();
        let match_case = self.search_state.match_case;
        let match_whole_word = self.search_state.match_whole_word;
        if query == self.search_state.last_search_query
            && match_case == self.search_state.last_search_match_case
            && match_whole_word == self.search_state.last_search_match_whole_word
            && !self.search_state.search_matches.is_empty()
        {
            return;
        }
        self.search_state.last_search_query.clone_from(&query);
        self.search_state.last_search_match_case = match_case;
        self.search_state.last_search_match_whole_word = match_whole_word;
        self.search_state.search_matches.clear();
        self.search_state.current_match_index = None;
        if let Some(active_index) = self.active_tab_index
            && let Some(content_entity) = self
                .tabs
                .get(active_index)
                .and_then(|tab| tab.as_editor().map(|editor_tab| editor_tab.content.clone()))
        {
            content_entity.update(cx, |content, _cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    diagnostics.clear();
                }
            });
            if query.is_empty() {
                cx.notify();
                return;
            }
            let mut search_text_scratch =
                std::mem::take(&mut self.search_state.search_text_scratch);
            let cursor_pos = {
                let content = content_entity.read(cx);
                search_text_scratch.clear();
                for chunk in content.text().chunks() {
                    search_text_scratch.push_str(chunk);
                }
                content.cursor()
            };
            let matches = find_matches_with_scratch(
                search_text_scratch.as_str(),
                &query,
                match_case,
                match_whole_word,
                &mut self.search_state.search_newline_offsets_scratch,
                &mut self.search_state.search_lowercase_text_scratch,
                &mut self.search_state.search_lowercase_offsets_scratch,
            );
            self.search_state.search_text_scratch = search_text_scratch;
            self.search_state.search_matches = matches;
            content_entity.update(cx, |content, cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    for search_match in &self.search_state.search_matches {
                        let diagnostic = Diagnostic {
                            range: lsp_types::Range {
                                start: Position {
                                    line: u32::try_from(search_match.line).unwrap_or(u32::MAX),
                                    character: u32::try_from(search_match.col).unwrap_or(u32::MAX),
                                },
                                end: Position {
                                    line: u32::try_from(search_match.line).unwrap_or(u32::MAX),
                                    character: u32::try_from(
                                        search_match.col + (search_match.end - search_match.start),
                                    )
                                    .unwrap_or(u32::MAX),
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
                        line: u32::try_from(search_match.line).unwrap_or(u32::MAX),
                        character: u32::try_from(search_match.col).unwrap_or(u32::MAX),
                    },
                    window,
                    cx,
                );
            });
        }
    }

    /// Force a fresh search, bypassing the query/option dedup cache
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    fn force_perform_search(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.search_state.last_search_query.clear();
        self.search_state.search_matches.clear();
        self.perform_search(window, cx);
    }

    /// Replace the current search match
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn replace_current(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Recompute matches against the current buffer before slicing: the cached
        // offsets may be stale if the document was edited since the last search.
        self.force_perform_search(window, cx);
        if let Some(match_index) = self.search_state.current_match_index
            && let Some(search_match) = self.search_state.search_matches.get(match_index).cloned()
            && let Some(active_index) = self.active_tab_index
            && let Some(tab) = self.tabs.get_mut(active_index)
            && let Some(editor_tab) = tab.as_editor_mut()
        {
            let replace_text = self.search_state.replace_input.read(cx).text().to_string();
            let text = editor_tab.content.read(cx).text().to_string();
            // Defensive guard against stale offsets: bail out instead of slicing
            // out of bounds or on a non-char-boundary if the buffer changed.
            if search_match.end > text.len()
                || search_match.start > search_match.end
                || !text.is_char_boundary(search_match.start)
                || !text.is_char_boundary(search_match.end)
            {
                cx.notify();
                return;
            }
            let mut new_text = String::new();
            new_text.push_str(&text[..search_match.start]);
            new_text.push_str(&replace_text);
            new_text.push_str(&text[search_match.end..]);
            editor_tab.content.update(cx, |content, cx| {
                content.set_value(&new_text, window, cx);
            });
            self.search_state.search_matches.clear();
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
        // Recompute matches against the current buffer before slicing: the cached
        // offsets may be stale if the document was edited since the last search.
        self.force_perform_search(window, cx);
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
                let new_text = if match_case && !match_whole_word {
                    text.replace(&search_query, &replace_text)
                } else {
                    apply_replacements(&self.search_state.search_matches, &text, &replace_text)
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
