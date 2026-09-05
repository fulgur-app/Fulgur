use gpui::{App, Context, Entity, HighlightStyle};
use gpui_component::{
    ActiveTheme,
    input::{EditorState, TextDecoration, TextDecorationCollection},
};

use super::SearchBar;
use super::state::MatchDecorations;

/// Opacity applied to the theme selection color for the non-current matches
const ALL_MATCHES_ALPHA: f32 = 0.28;

/// Opacity applied to the theme selection color for the current match
const CURRENT_MATCH_ALPHA: f32 = 0.62;

impl SearchBar {
    /// Repaint the search highlights, accenting the current match
    ///
    /// ### Arguments
    /// - `content`: The editor content the matches were found in
    /// - `cx`: The application context
    pub(super) fn apply_match_decorations(&mut self, content: &Entity<EditorState>, cx: &mut App) {
        let active_editor_id = content.entity_id();
        for (editor_id, decorations) in &self.match_decorations {
            if *editor_id != active_editor_id {
                decorations.all.clear(cx);
                decorations.current.clear(cx);
            }
        }
        if self.search_matches.is_empty() {
            if let Some(decorations) = self.match_decorations.get(&active_editor_id) {
                decorations.all.clear(cx);
                decorations.current.clear(cx);
            }
            return;
        }
        let (all, current) = self.decorations_for(content, cx);
        let selection = cx.theme().selection;
        let all_style = HighlightStyle {
            background_color: Some(selection.alpha(ALL_MATCHES_ALPHA)),
            ..Default::default()
        };
        let current_style = HighlightStyle {
            background_color: Some(selection.alpha(CURRENT_MATCH_ALPHA)),
            ..Default::default()
        };
        let current_range = self
            .current_match_index
            .and_then(|index| self.search_matches.get(index))
            .map(|search_match| search_match.start..search_match.end);
        let other_matches = self
            .search_matches
            .iter()
            .map(|search_match| search_match.start..search_match.end)
            .filter(|range| Some(range) != current_range.as_ref())
            .map(|range| TextDecoration::new(range, all_style))
            .collect();
        all.set(other_matches, cx);
        current.set(
            current_range
                .map(|range| vec![TextDecoration::new(range, current_style)])
                .unwrap_or_default(),
            cx,
        );
    }

    /// Repaint the search highlights of the active editor with the current theme
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(crate) fn refresh_match_decorations(&mut self, cx: &mut Context<Self>) {
        if !self.show_search {
            return;
        }
        if let Some(content) = self.active_editor_content(cx) {
            self.apply_match_decorations(&content, cx);
        }
    }

    /// Remove the search highlights from every editor they were painted in
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(super) fn clear_match_decorations(&self, cx: &mut App) {
        for decorations in self.match_decorations.values() {
            decorations.all.clear(cx);
            decorations.current.clear(cx);
        }
    }

    /// Get the decoration collections of an editor, creating them on first use
    ///
    /// ### Arguments
    /// - `content`: The editor content to decorate
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `(TextDecorationCollection, TextDecorationCollection)`: The collections
    ///   for all the non-current matches and for the current match
    fn decorations_for(
        &mut self,
        content: &Entity<EditorState>,
        cx: &mut App,
    ) -> (TextDecorationCollection, TextDecorationCollection) {
        let editor_id = content.entity_id();
        if let Some(decorations) = self.match_decorations.get(&editor_id) {
            return (decorations.all.clone(), decorations.current.clone());
        }
        self.match_decorations
            .retain(|_, decorations| decorations.editor.upgrade().is_some());
        let (all, current) = content.update(cx, |content, cx| {
            (
                content.create_decorations_collection(Vec::new(), cx),
                content.create_decorations_collection(Vec::new(), cx),
            )
        });
        self.match_decorations.insert(
            editor_id,
            MatchDecorations {
                editor: content.downgrade(),
                all: all.clone(),
                current: current.clone(),
            },
        );
        (all, current)
    }
}
