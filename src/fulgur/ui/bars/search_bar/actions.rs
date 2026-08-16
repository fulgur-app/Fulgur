use crate::fulgur::Fulgur;
use gpui::{App, Context, Entity, Focusable, Window};
use gpui_component::input::{EditorState, Position};
use lsp_types::{Diagnostic, DiagnosticSeverity};

use super::matching::find_matches_with_scratch;
use super::{SearchBar, SearchBarEvent};

impl SearchBar {
    /// Toggle the search bar open or closed
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn toggle(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.show_search {
            self.close(content, cx);
        } else {
            self.show_search = true;
            let search_focus = self.search_input.read(cx).focus_handle(cx);
            window.focus(&search_focus, cx);
            self.perform_search(content, window, cx);
            cx.notify();
        }
    }

    /// Close the search bar, clear highlighting, and notify the owning window
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content to clear highlighting from, if any
    /// - `cx`: The search bar context
    pub(super) fn close(&mut self, content: Option<Entity<EditorState>>, cx: &mut Context<Self>) {
        self.show_search = false;
        if let Some(content) = content {
            content.update(cx, |content, _cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    diagnostics.clear();
                }
            });
        }
        self.search_matches.clear();
        self.current_match_index = None;
        cx.emit(SearchBarEvent::Closed);
        cx.notify();
    }

    /// Re-run the search after the query text changed
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn on_query_changed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self.active_editor_content(cx);
        self.perform_search(content, window, cx);
        let search_focus = self.search_input.read(cx).focus_handle(cx);
        window.focus(&search_focus, cx);
    }

    /// Clear the current matches and search the given editor content afresh
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(crate) fn refresh_matches(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.search_matches.clear();
        self.perform_search(content, window, cx);
    }

    /// Perform search in the given editor content
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn perform_search(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let query = self.search_input.read(cx).text().to_string();
        let match_case = self.match_case;
        let match_whole_word = self.match_whole_word;
        if query == self.last_search_query
            && match_case == self.last_search_match_case
            && match_whole_word == self.last_search_match_whole_word
            && !self.search_matches.is_empty()
        {
            return;
        }
        self.last_search_query.clone_from(&query);
        self.last_search_match_case = match_case;
        self.last_search_match_whole_word = match_whole_word;
        self.search_matches.clear();
        self.current_match_index = None;
        if let Some(content_entity) = content {
            content_entity.update(cx, |content, _cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    diagnostics.clear();
                }
            });
            if query.is_empty() {
                cx.notify();
                return;
            }
            let mut search_text_scratch = std::mem::take(&mut self.search_text_scratch);
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
                &mut self.search_newline_offsets_scratch,
                &mut self.search_lowercase_text_scratch,
                &mut self.search_lowercase_offsets_scratch,
            );
            self.search_text_scratch = search_text_scratch;
            self.search_matches = matches;
            content_entity.update(cx, |content, cx| {
                if let Some(diagnostics) = content.diagnostics_mut() {
                    for search_match in &self.search_matches {
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
                self.highlight_current_match(&content_entity, window, cx);
            }
        }

        cx.notify();
    }

    /// Navigate to the next search match
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn search_next(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(current) = self.current_match_index {
            self.current_match_index = Some((current + 1) % self.search_matches.len());
        } else {
            self.current_match_index = Some(0);
        }
        if let Some(content) = content {
            self.highlight_current_match(&content, window, cx);
        }
        cx.notify();
    }

    /// Navigate to the previous search match
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn search_previous(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
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
        if let Some(content) = content {
            self.highlight_current_match(&content, window, cx);
        }
        cx.notify();
    }

    /// Move the editor cursor to the current search match
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(super) fn highlight_current_match(
        &self,
        content: &Entity<EditorState>,
        window: &mut Window,
        cx: &mut App,
    ) {
        if let Some(match_index) = self.current_match_index
            && let Some(search_match) = self.search_matches.get(match_index)
        {
            content.update(cx, |content, cx| {
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
}

impl Fulgur {
    /// Toggle the search bar in this window
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn find_in_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let content = self
            .get_active_editor_tab(cx)
            .map(|editor_tab| editor_tab.content.clone());
        self.search_bar
            .update(cx, |bar, cx| bar.toggle(content, window, cx));
        cx.notify();
    }
}
