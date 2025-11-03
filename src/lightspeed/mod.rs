mod components_utils;
mod editor_tab;
mod file_operations;
mod languages;
mod menus;
mod search_bar;
mod search_replace;
mod status_bar;
mod tab_bar;
mod tab_manager;
mod themes;
mod titlebar;

use editor_tab::EditorTab;
use menus::*;
use search_replace::SearchMatch;
use titlebar::CustomTitleBar;

use gpui::*;
use gpui_component::{
    Root, StyledExt, Theme, ThemeRegistry,
    input::{InputEvent, InputState, TextInput},
};

pub struct Lightspeed {
    focus_handle: FocusHandle,
    title_bar: Entity<CustomTitleBar>,
    tabs: Vec<EditorTab>,
    active_tab_index: Option<usize>,
    next_tab_id: usize,
    show_search: bool,
    search_input: Entity<InputState>,
    replace_input: Entity<InputState>,
    match_case: bool,
    match_whole_word: bool,
    search_matches: Vec<SearchMatch>,
    current_match_index: Option<usize>,
    _search_subscription: gpui::Subscription,
    last_search_query: String,
}

impl Lightspeed {
    /// Create a new Lightspeed instance
    /// @param window: The window to create the Lightspeed instance in
    /// @param cx: The application context
    /// @return: The new Lightspeed instance
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let title_bar = CustomTitleBar::new(window, cx);

        // Create initial tab
        let initial_tab = EditorTab::new(0, "Untitled", window, cx);

        // Create inputs
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));

        cx.new(|cx| {
            // Subscribe to search input changes for auto-search
            let _search_subscription =
                cx.subscribe(&search_input, |this: &mut Self, _, ev: &InputEvent, cx| {
                    match ev {
                        InputEvent::Change => {
                            // Auto-search when user types (will be triggered on next render)
                            if this.show_search {
                                cx.notify();
                            }
                        }
                        _ => {}
                    }
                });

            let entity = Self {
                focus_handle: cx.focus_handle(),
                title_bar,
                tabs: vec![initial_tab],
                active_tab_index: Some(0),
                next_tab_id: 1,
                show_search: false,
                search_input,
                replace_input,
                match_case: false,
                match_whole_word: false,
                search_matches: Vec::new(),
                current_match_index: None,
                _search_subscription,
                last_search_query: String::new(),
            };
            entity
        })
    }

    /// Initialize the Lightspeed instance
    /// @param cx: The application context
    pub fn init(cx: &mut App) {
        // Initialize language support for syntax highlighting
        languages::init_languages();

        themes::init(cx, |cx| {
            // Set up keyboard shortcuts
            cx.bind_keys([
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-o", OpenFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-o", OpenFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-n", NewFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-n", NewFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-w", CloseFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-w", CloseFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-w", CloseAllFiles, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-w", CloseAllFiles, None),
                KeyBinding::new("cmd-q", Quit, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-q", Quit, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-s", SaveFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-s", SaveFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-s", SaveFileAs, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-s", SaveFileAs, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-f", FindInFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-f", FindInFile, None),
            ]);

            let menus = build_menus(cx);
            cx.set_menus(menus);
        });
    }
}

impl Focusable for Lightspeed {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Lightspeed {
    /// Render the Lightspeed instance
    /// @param window: The window to render the Lightspeed instance in
    /// @param cx: The application context
    /// @return: The rendered Lightspeed instance
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Ensure we always have at least one tab
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        }

        // Auto-search when query changes
        if self.show_search {
            let current_query = self.search_input.read(cx).text().to_string();
            if current_query != self.last_search_query {
                self.last_search_query = current_query;
                self.perform_search(window, cx);
            }
        }

        // Update modified status of tabs
        self.update_modified_status(cx);

        let active_tab = match self.active_tab_index {
            Some(index) => Some(self.tabs[index].clone()),
            None => None,
        };

        // Render modal, drawer, and notification layers
        let modal_layer = Root::render_modal_layer(window, cx);
        let drawer_layer = Root::render_drawer_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        let main_div = div()
            .size_full()
            .child(
                div()
                    .size_full()
                    .v_flex()
                    .track_focus(&self.focus_handle)
                    .on_action(cx.listener(|this, _action: &NewFile, window, cx| {
                        this.new_tab(window, cx);
                    }))
                    .on_action(cx.listener(|this, _action: &OpenFile, window, cx| {
                        this.open_file(window, cx);
                    }))
                    .on_action(cx.listener(|this, _action: &CloseFile, window, cx| {
                        if let Some(index) = this.active_tab_index {
                            this.close_tab(index, window, cx);
                        }
                    }))
                    .on_action(cx.listener(|this, _action: &CloseAllFiles, window, cx| {
                        this.close_all_tabs(window, cx);
                    }))
                    .on_action(cx.listener(|this, _action: &SaveFile, window, cx| {
                        this.save_file(window, cx);
                    }))
                    .on_action(cx.listener(|this, _action: &SaveFileAs, window, cx| {
                        this.save_file_as(window, cx);
                    }))
                    .on_action(cx.listener(|this, _action: &Quit, window, cx| {
                        this.quit(window, cx);
                    }))
                    .on_action(cx.listener(|this, _action: &FindInFile, window, cx| {
                        this.show_search = !this.show_search;

                        if this.show_search {
                            // Focus the search input when opening
                            let search_focus = this.search_input.read(cx).focus_handle(cx);
                            window.focus(&search_focus);

                            // Perform search with current query if any
                            this.perform_search(window, cx);
                        } else {
                            // Close search and clear highlighting
                            this.close_search(window, cx);
                        }

                        cx.notify();
                    }))
                    .on_action(cx.listener(|_this, _action: &SwitchTheme, _window, cx| {
                        let theme_name = _action.0.clone();
                        if let Some(theme_config) =
                            ThemeRegistry::global(cx).themes().get(&theme_name).cloned()
                        {
                            Theme::global_mut(cx).apply_config(&theme_config);
                        }
                        cx.refresh_windows();
                    }))
                    .child(self.title_bar.clone())
                    .child(self.render_tab_bar(window, cx))
                    .child(self.render_content_area(active_tab))
                    .children(self.render_search_bar(window, cx))
                    .child(self.render_status_bar(window, cx)),
            )
            .children(drawer_layer)
            .children(modal_layer)
            .children(notification_layer);

        main_div
    }
}

impl Lightspeed {
    /// Render the content area (editor)
    /// @param active_tab: The active tab (if any)
    /// @return: The rendered content area element
    fn render_content_area(&self, active_tab: Option<EditorTab>) -> Div {
        let mut content_div = div().flex_1().p_0().m_0().overflow_hidden();

        if let Some(tab) = active_tab {
            content_div = content_div.child(
                TextInput::new(&tab.content)
                    .w_full()
                    .h_full()
                    .border_0()
                    .text_size(px(14.)),
            );
        }

        content_div
    }
}
