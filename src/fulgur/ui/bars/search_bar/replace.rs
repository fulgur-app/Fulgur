use gpui::{Context, Entity, Window};
use gpui_component::input::InputState;

use super::SearchBar;
use super::matching::apply_replacements;

impl SearchBar {
    /// Force a fresh search, bypassing the query/option dedup cache
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    fn force_perform_search(
        &mut self,
        content: Option<Entity<InputState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.last_search_query.clear();
        self.search_matches.clear();
        self.perform_search(content, window, cx);
    }

    /// Replace the current search match
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn replace_current(
        &mut self,
        content: Option<Entity<InputState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Recompute matches against the current buffer before slicing: the cached
        // offsets may be stale if the document was edited since the last search.
        self.force_perform_search(content.clone(), window, cx);
        if let Some(match_index) = self.current_match_index
            && let Some(search_match) = self.search_matches.get(match_index).cloned()
            && let Some(content_entity) = content
        {
            let replace_text = self.replace_input.read(cx).text().to_string();
            let text = content_entity.read(cx).text().to_string();
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
            content_entity.update(cx, |content, cx| {
                content.set_value(&new_text, window, cx);
            });
            self.search_matches.clear();
            self.perform_search(Some(content_entity.clone()), window, cx);
            if !self.search_matches.is_empty() {
                if match_index < self.search_matches.len() {
                    self.current_match_index = Some(match_index);
                } else {
                    self.current_match_index = Some(0);
                }
                self.highlight_current_match(&content_entity, window, cx);
            }
        }
        cx.notify();
    }

    /// Replace all search matches
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn replace_all(
        &mut self,
        content: Option<Entity<InputState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.force_perform_search(content.clone(), window, cx);
        if self.search_matches.is_empty() {
            return;
        }
        if let Some(content_entity) = content {
            let replace_text = self.replace_input.read(cx).text().to_string();
            let search_query = self.search_input.read(cx).text().to_string();
            let match_case = self.match_case;
            let match_whole_word = self.match_whole_word;
            let text = content_entity.read(cx).text().to_string();
            let new_text = if match_case && !match_whole_word {
                text.replace(&search_query, &replace_text)
            } else {
                apply_replacements(&self.search_matches, &text, &replace_text)
            };
            content_entity.update(cx, |content, cx| {
                content.set_value(&new_text, window, cx);
            });
            self.search_matches.clear();
            self.current_match_index = None;
        }
        cx.notify();
    }
}
