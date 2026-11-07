use gpui_kit::component::input::EditorState;
use gpui_kit::{App, Context, Entity, Window};

use super::SearchBar;
use super::matching::{apply_replacements, offset_after_replacements};

impl SearchBar {
    /// Return whether a buffer accepts edits from the search bar.
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when there is a buffer and it is editable by the user
    fn content_is_editable(content: Option<&Entity<EditorState>>, cx: &App) -> bool {
        content.is_some_and(|content| content.read(cx).is_editable())
    }

    /// Replace the current search match
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn replace_current(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !Self::content_is_editable(content.as_ref(), cx) {
            return;
        }
        // Recompute matches against the current buffer before slicing: the cached
        // offsets may be stale if the document was edited since the last search.
        self.perform_search(content.clone(), cx);
        if let Some(match_index) = self.current_match_index
            && let Some(search_match) = self.search_matches.get(match_index).cloned()
            && let Some(content_entity) = content
        {
            let replace_text = self.replace_input.read(cx).text().to_string();
            content_entity.update(cx, |content, cx| {
                content.set_selected_range(search_match.start..search_match.end, cx);
                content.replace(replace_text, window, cx);
            });
            self.perform_search(Some(content_entity.clone()), cx);
            if !self.search_matches.is_empty() {
                if match_index < self.search_matches.len() {
                    self.current_match_index = Some(match_index);
                } else {
                    self.current_match_index = Some(0);
                }
                self.scroll_to_current_match(&content_entity, cx);
            }
            self.apply_match_decorations(&content_entity, cx);
        }
        cx.notify();
    }

    /// Replace all search matches, keeping the caret on its text and the view where it is
    ///
    /// ### Arguments
    /// - `content`: The active editor tab's content, if any
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(super) fn replace_all(
        &mut self,
        content: Option<Entity<EditorState>>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !Self::content_is_editable(content.as_ref(), cx) {
            return;
        }
        self.rescan_matches(content.as_ref(), cx);
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
            let search_matches = &self.search_matches;
            content_entity.update(cx, |content, cx| {
                let caret =
                    offset_after_replacements(search_matches, content.cursor(), replace_text.len());
                let scroll_offset = content.scroll_offset();
                content.replace_all(&new_text, window, cx);
                content.set_selected_range(caret..caret, cx);
                // Last: moving the caret also requests a scroll, and the last request wins.
                content.set_scroll_offset(scroll_offset, cx);
            });
            self.search_matches.clear();
            self.current_match_index = None;
            self.clear_match_decorations(cx);
        }
        cx.notify();
    }
}
