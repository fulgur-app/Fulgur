use super::SearchMatch;
use super::field::new_field;
use gpui_kit::component::input::{
    EditorState, InputEvent, TextDecorationCollection, TextareaState,
};
use gpui_kit::{App, Context, Entity, EventEmitter, Subscription, Task, WeakEntity, Window};

/// The pair of decoration collections painting the search matches of the bar's editor
///
/// `all` carries every match and `current` overlays the current one, so stepping
/// through the matches only rewrites `current`. `current` must be created first:
/// the first collection wins when overlapping decorations set the same property.
pub(super) struct MatchDecorations {
    pub(super) all: TextDecorationCollection,
    pub(super) current: TextDecorationCollection,
}

/// The search and replace bar of one editor tab, rendered as its own entity
///
/// ### Description
/// Each editor tab creates its bar on the first Find and drops it with the tab, so
/// the query, options and matches belong to that tab alone.
pub(crate) struct SearchBar {
    pub(super) editor: WeakEntity<EditorState>,
    pub(super) show_search: bool,
    pub(super) search_input: Entity<TextareaState>,
    pub(super) replace_input: Entity<TextareaState>,
    pub(super) match_case: bool,
    pub(super) match_whole_word: bool,
    pub(super) search_matches: Vec<SearchMatch>,
    pub(super) current_match_index: Option<usize>,
    pub(super) match_decorations: Option<MatchDecorations>,
    pub(super) editor_subscription: Option<Subscription>,
    pub(super) pending_rescan: Option<Task<()>>,
    _search_input_subscription: Subscription,
}

/// Typed events emitted by the search bar toward the owning `Fulgur` window
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchBarEvent {
    Closed,
}

impl EventEmitter<SearchBarEvent> for SearchBar {}

/// The plain-data state of a search bar, carried when its tab moves to another window
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct SearchBarSnapshot {
    pub visible: bool,
    pub query: String,
    pub replacement: String,
    pub match_case: bool,
    pub match_whole_word: bool,
}

/// A search bar stored on its editor tab, with the subscription routing its events to the window
pub(crate) struct TabSearchBar {
    pub(crate) bar: Entity<SearchBar>,
    _events: Subscription,
}

impl TabSearchBar {
    /// Pair a search bar with the subscription delivering its events
    ///
    /// ### Arguments
    /// - `bar`: The tab's search bar
    /// - `events`: The subscription routing the bar's events to the owning window
    ///
    /// ### Returns
    /// - `TabSearchBar`: The bar ready to be stored on its tab
    pub(crate) fn new(bar: Entity<SearchBar>, events: Subscription) -> Self {
        Self {
            bar,
            _events: events,
        }
    }
}

impl SearchBar {
    /// Create a new search bar view searching one editor
    ///
    /// Subscribes to its own search input so the search re-runs whenever the
    /// query changes while the bar is visible.
    ///
    /// ### Arguments
    /// - `editor`: Weak handle to the content of the tab owning the bar
    /// - `window`: The window context
    /// - `cx`: The search bar context
    ///
    /// ### Returns
    /// - `SearchBar`: The new search bar view
    pub(crate) fn new(
        editor: WeakEntity<EditorState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let search_input = new_field("Search", window, cx);
        let replace_input = new_field("Replace", window, cx);
        let search_input_subscription =
            cx.subscribe(&search_input, |this: &mut Self, _, ev: &InputEvent, cx| {
                if this.show_search && matches!(ev, InputEvent::Change) {
                    this.on_query_changed(cx);
                }
            });
        Self {
            editor,
            show_search: false,
            search_input,
            replace_input,
            match_case: false,
            match_whole_word: false,
            search_matches: Vec::new(),
            current_match_index: None,
            match_decorations: None,
            editor_subscription: None,
            pending_rescan: None,
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

    /// Get the content of the tab owning the bar
    ///
    /// ### Returns
    /// - `Some(Entity<EditorState>)`: The editor the bar searches
    /// - `None`: If the tab, and so its editor, is gone
    pub(super) fn editor(&self) -> Option<Entity<EditorState>> {
        self.editor.upgrade()
    }

    /// Return whether the bar's editor accepts user edits.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the editor is alive and editable
    pub(super) fn editor_is_editable(&self, cx: &App) -> bool {
        self.editor()
            .is_some_and(|editor| editor.read(cx).is_editable())
    }

    /// Capture the state worth keeping when the tab moves to another window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `SearchBarSnapshot`: The visibility, query, replacement text and options
    pub(crate) fn snapshot(&self, cx: &App) -> SearchBarSnapshot {
        SearchBarSnapshot {
            visible: self.show_search,
            query: self.search_input.read(cx).value().to_string(),
            replacement: self.replace_input.read(cx).value().to_string(),
            match_case: self.match_case,
            match_whole_word: self.match_whole_word,
        }
    }

    /// Restore a captured state, searching again without moving the caret or the focus
    ///
    /// ### Arguments
    /// - `snapshot`: The state captured in the source window
    /// - `window`: The window context
    /// - `cx`: The search bar context
    pub(crate) fn restore(
        &mut self,
        snapshot: SearchBarSnapshot,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.match_case = snapshot.match_case;
        self.match_whole_word = snapshot.match_whole_word;
        self.search_input.update(cx, |input, cx| {
            input.set_value(snapshot.query, window, cx);
        });
        self.replace_input.update(cx, |input, cx| {
            input.set_value(snapshot.replacement, window, cx);
        });
        if snapshot.visible {
            self.show_search = true;
            self.watch_editor(cx);
            self.rescan_matches(cx);
        }
        cx.notify();
    }
}
