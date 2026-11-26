//! The window side of the per-tab search bars: Find actions, event routing and repaints.

use super::actions::MatchStep;
use super::{SearchBar, SearchBarEvent, TabSearchBar};
use crate::fulgur::{Fulgur, tab::TabId};
use gpui_kit::{App, AppContext, Context, Entity, Window};

impl Fulgur {
    /// Get the search bar of the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<SearchBar>)`: The bar of the active editor tab, shown or not
    /// - `None`: If the active tab is not an editor tab or never opened its bar
    #[must_use]
    pub(crate) fn active_search_bar(&self, cx: &App) -> Option<Entity<SearchBar>> {
        self.get_active_editor_tab(cx)?
            .search_bar
            .as_ref()
            .map(|tab_bar| tab_bar.bar.clone())
    }

    /// Whether the active tab currently shows its search bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the active editor tab has a visible search bar
    #[must_use]
    pub(crate) fn active_search_bar_visible(&self, cx: &App) -> bool {
        self.active_search_bar(cx)
            .is_some_and(|bar| bar.read(cx).is_visible())
    }

    /// Get the search bar of an editor tab, creating it on first use
    ///
    /// ### Arguments
    /// - `tab_id`: The tab whose bar is needed
    /// - `window`: The window holding the tab
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<SearchBar>)`: The tab's bar
    /// - `None`: If no editor tab has this identifier, so there is nothing to search
    pub(crate) fn ensure_search_bar(
        &mut self,
        tab_id: TabId,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<Entity<SearchBar>> {
        let editor_tab = self.editor_tab(tab_id, cx)?;
        if let Some(tab_bar) = &editor_tab.search_bar {
            return Some(tab_bar.bar.clone());
        }
        let editor = editor_tab.content.downgrade();
        let bar = cx.new(|cx| SearchBar::new(editor, window, cx));
        let events = cx.subscribe_in(
            &bar,
            window,
            |this: &mut Self, bar, event: &SearchBarEvent, window, cx| {
                this.on_search_bar_event(bar, *event, window, cx);
            },
        );
        let tab_bar = TabSearchBar::new(bar.clone(), events);
        self.update_editor_tab(tab_id, cx, |editor_tab, _| {
            editor_tab.search_bar = Some(tab_bar);
        });
        Some(bar)
    }

    /// Toggle the search bar of the active tab
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn find_in_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab_id) = self.active_tab_id
            && let Some(bar) = self.ensure_search_bar(tab_id, window, cx)
        {
            bar.update(cx, |bar, cx| bar.toggle(window, cx));
            cx.notify();
        }
    }

    /// Open the search bar of the active tab with the caret in the replace field
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn find_and_replace(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(tab_id) = self.active_tab_id
            && let Some(bar) = self.ensure_search_bar(tab_id, window, cx)
        {
            bar.update(cx, |bar, cx| bar.open_replace(window, cx));
            cx.notify();
        }
    }

    /// Move to the next search match of the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn find_next(&mut self, cx: &mut Context<Self>) {
        self.step_search_match(MatchStep::Next, cx);
    }

    /// Move to the previous search match of the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn find_previous(&mut self, cx: &mut Context<Self>) {
        self.step_search_match(MatchStep::Previous, cx);
    }

    /// Step the active tab's search bar through its matches
    ///
    /// ### Arguments
    /// - `step`: Which way to move through the matches
    /// - `cx`: The application context
    fn step_search_match(&mut self, step: MatchStep, cx: &mut Context<Self>) {
        if let Some(bar) = self.active_search_bar(cx) {
            bar.update(cx, |bar, cx| bar.step_match(step, cx));
        }
    }

    /// Repaint the search highlights of every tab of this window with the current theme
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub(crate) fn refresh_search_decorations(&self, cx: &mut App) {
        let bars: Vec<Entity<SearchBar>> = self
            .tabs
            .iter()
            .filter_map(|tab| tab.read(cx).as_editor()?.search_bar.as_ref())
            .map(|tab_bar| tab_bar.bar.clone())
            .collect();
        for bar in bars {
            bar.update(cx, SearchBar::refresh_match_decorations);
        }
    }

    /// Dispatch a search bar event to the matching window-level handler
    ///
    /// ### Arguments
    /// - `bar`: The bar that emitted the event
    /// - `event`: The search bar event to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn on_search_bar_event(
        &mut self,
        bar: &Entity<SearchBar>,
        event: SearchBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchBarEvent::Closed => {
                if self.active_search_bar(cx).as_ref() == Some(bar) {
                    self.focus_active_tab(window, cx);
                }
                cx.notify();
            }
        }
    }
}
