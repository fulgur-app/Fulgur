use gpui_kit::component::{
    ActiveTheme,
    input::{EditorState, TextDecoration, TextDecorationCollection},
};
use gpui_kit::{App, Context, Entity, HighlightStyle};

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
    /// - `editor`: The bar's editor, which the matches were found in
    /// - `cx`: The application context
    pub(super) fn apply_match_decorations(&mut self, editor: &Entity<EditorState>, cx: &mut App) {
        if self.search_matches.is_empty() {
            self.clear_match_decorations(cx);
            return;
        }
        let (all, _) = self.decorations_for(editor, cx);
        let all_style = HighlightStyle {
            background_color: Some(cx.theme().selection.alpha(ALL_MATCHES_ALPHA)),
            ..Default::default()
        };
        let every_match = self
            .search_matches
            .iter()
            .map(|search_match| {
                TextDecoration::new(search_match.start..search_match.end, all_style)
            })
            .collect();
        all.set(every_match, cx);
        self.apply_current_match_decoration(editor, cx);
    }

    /// Move the accent onto the current match, leaving the other highlights untouched
    ///
    /// ### Arguments
    /// - `editor`: The bar's editor, which the matches were found in
    /// - `cx`: The application context
    pub(super) fn apply_current_match_decoration(
        &mut self,
        editor: &Entity<EditorState>,
        cx: &mut App,
    ) {
        let (_, current) = self.decorations_for(editor, cx);
        let current_style = HighlightStyle {
            background_color: Some(cx.theme().selection.alpha(CURRENT_MATCH_ALPHA)),
            ..Default::default()
        };
        let current_match = self
            .current_match_index
            .and_then(|index| self.search_matches.get(index))
            .map(|search_match| {
                TextDecoration::new(search_match.start..search_match.end, current_style)
            });
        current.set(current_match.into_iter().collect(), cx);
    }

    /// Repaint the search highlights with the current theme
    ///
    /// ### Description
    /// Called for every open bar of a window, active tab or not, so returning to a
    /// background tab never shows the colors of a previous theme.
    ///
    /// ### Arguments
    /// - `cx`: The search bar context
    pub(crate) fn refresh_match_decorations(&mut self, cx: &mut Context<Self>) {
        if !self.show_search {
            return;
        }
        if let Some(editor) = self.editor() {
            self.apply_match_decorations(&editor, cx);
        }
    }

    /// Remove the search highlights from the editor
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(super) fn clear_match_decorations(&self, cx: &mut App) {
        if let Some(decorations) = &self.match_decorations {
            decorations.all.clear(cx);
            decorations.current.clear(cx);
        }
    }

    /// Get the decoration collections of the editor, creating them on first use
    ///
    /// ### Arguments
    /// - `editor`: The bar's editor
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `(TextDecorationCollection, TextDecorationCollection)`: The collections
    ///   for all the non-current matches and for the current match
    fn decorations_for(
        &mut self,
        editor: &Entity<EditorState>,
        cx: &mut App,
    ) -> (TextDecorationCollection, TextDecorationCollection) {
        if let Some(decorations) = &self.match_decorations {
            return (decorations.all.clone(), decorations.current.clone());
        }
        let (current, all) = editor.update(cx, |content, cx| {
            let current = content.create_decorations_collection(Vec::new(), cx);
            let all = content.create_decorations_collection(Vec::new(), cx);
            (current, all)
        });
        self.match_decorations = Some(MatchDecorations {
            all: all.clone(),
            current: current.clone(),
        });
        (all, current)
    }
}
