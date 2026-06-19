use super::SearchMatch;
use gpui::Entity;
use gpui_component::input::InputState;

/// Search and replace functionality state
///
/// This struct groups all state related to the search/replace feature.
/// It manages the search UI state, search results, and the subscription
/// to search input changes.
pub struct SearchState {
    pub show_search: bool,
    pub search_input: Entity<InputState>,
    pub replace_input: Entity<InputState>,
    pub match_case: bool,
    pub match_whole_word: bool,
    pub search_matches: Vec<SearchMatch>,
    pub current_match_index: Option<usize>,
    pub last_search_query: String,
    pub last_search_match_case: bool,
    pub last_search_match_whole_word: bool,
    pub search_subscription: gpui::Subscription,
    pub search_text_scratch: String,
    pub search_newline_offsets_scratch: Vec<usize>,
    pub search_lowercase_text_scratch: String,
    pub search_lowercase_offsets_scratch: Vec<usize>,
}

impl SearchState {
    /// Create a new `SearchState`
    ///
    /// ### Arguments
    /// - `search_input`: The search input entity
    /// - `replace_input`: The replace input entity
    /// - `search_subscription`: The subscription to search input changes
    ///
    /// ### Returns
    /// `Self`: A new `SearchState` instance with default values
    pub fn new(
        search_input: Entity<InputState>,
        replace_input: Entity<InputState>,
        search_subscription: gpui::Subscription,
    ) -> Self {
        Self {
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
            search_subscription,
            search_text_scratch: String::new(),
            search_newline_offsets_scratch: Vec::new(),
            search_lowercase_text_scratch: String::new(),
            search_lowercase_offsets_scratch: Vec::new(),
        }
    }
}
