use super::SearchMatch;
use crate::fulgur::Fulgur;
use gpui::{App, AppContext, Context, Entity, EventEmitter, Subscription, WeakEntity, Window};
use gpui_component::input::{InputEvent, InputState};

/// The search and replace bar, rendered as its own entity
///
pub(crate) struct SearchBar {
    pub(super) fulgur: WeakEntity<Fulgur>,
    pub(super) show_search: bool,
    pub(super) search_input: Entity<InputState>,
    pub(super) replace_input: Entity<InputState>,
    pub(super) match_case: bool,
    pub(super) match_whole_word: bool,
    pub(super) search_matches: Vec<SearchMatch>,
    pub(super) current_match_index: Option<usize>,
    pub(super) last_search_query: String,
    pub(super) last_search_match_case: bool,
    pub(super) last_search_match_whole_word: bool,
    pub(super) search_text_scratch: String,
    pub(super) search_newline_offsets_scratch: Vec<usize>,
    pub(super) search_lowercase_text_scratch: String,
    pub(super) search_lowercase_offsets_scratch: Vec<usize>,
    _search_input_subscription: Subscription,
}

/// Typed events emitted by the search bar toward the owning `Fulgur` window
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchBarEvent {
    Closed,
}

impl EventEmitter<SearchBarEvent> for SearchBar {}

impl SearchBar {
    /// Create a new search bar view owning its search and replace inputs
    ///
    /// Subscribes to its own search input so the search re-runs whenever the
    /// query changes while the bar is visible.
    ///
    /// ### Arguments
    /// - `fulgur`: Weak handle to the owning window entity the bar reads the active editor from
    /// - `window`: The window context
    /// - `cx`: The search bar context
    ///
    /// ### Returns
    /// - `SearchBar`: The new search bar view
    pub(crate) fn new(
        fulgur: WeakEntity<Fulgur>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
        let search_input_subscription = cx.subscribe_in(
            &search_input,
            window,
            |this: &mut Self, _, ev: &InputEvent, window, cx| {
                if let InputEvent::Change = ev
                    && this.show_search
                {
                    this.on_query_changed(window, cx);
                }
            },
        );
        Self {
            fulgur,
            show_search: false,
            search_input,
            replace_input,
            match_case: false,
            match_whole_word: false,
            search_matches: Vec::new(),
            current_match_index: None,
            last_search_query: String::new(),
            last_search_match_case: false,
            last_search_match_whole_word: false,
            search_text_scratch: String::new(),
            search_newline_offsets_scratch: Vec::new(),
            search_lowercase_text_scratch: String::new(),
            search_lowercase_offsets_scratch: Vec::new(),
            _search_input_subscription: search_input_subscription,
        }
    }

    /// Whether the search bar is currently shown
    ///
    /// ### Returns
    /// - `bool`: True if the bar is visible
    pub(crate) fn is_visible(&self) -> bool {
        self.show_search
    }

    /// Get the active editor's content entity from the owning window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<InputState>)`: The active editor tab's content
    /// - `None`: If the window is gone or the active tab is not an editor
    pub(super) fn active_editor_content(&self, cx: &App) -> Option<Entity<InputState>> {
        let fulgur = self.fulgur.upgrade()?;
        fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .map(|editor_tab| editor_tab.content.clone())
    }
}

impl Fulgur {
    /// Dispatch a search bar event to the matching window-level handler
    ///
    /// ### Arguments
    /// - `event`: The search bar event to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn on_search_bar_event(
        &mut self,
        event: SearchBarEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        match event {
            SearchBarEvent::Closed => {
                self.focus_active_tab(window, cx);
                cx.notify();
            }
        }
    }
}
