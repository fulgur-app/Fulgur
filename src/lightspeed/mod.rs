mod components_utils;
mod editor_tab;
mod file_operations;
mod languages;
mod menus;
mod search_bar;
mod search_replace;
mod settings;
mod state_operations;
mod state_persistence;
mod status_bar;
mod tab;
mod tab_bar;
mod tab_manager;
mod themes;
mod titlebar;

use menus::*;
use search_replace::SearchMatch;
use settings::Settings;
use tab::Tab;
use titlebar::CustomTitleBar;

use gpui::*;
use gpui_component::{
    Root, StyledExt, Theme, ThemeRegistry,
    dropdown::DropdownState,
    input::{InputEvent, InputState, TextInput},
};

pub struct Lightspeed {
    focus_handle: FocusHandle,
    title_bar: Entity<CustomTitleBar>,
    tabs: Vec<Tab>,
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
    pub settings: Settings,
    pub font_size_dropdown: Entity<DropdownState<Vec<SharedString>>>,
    _font_size_subscription: gpui::Subscription,
    pub tab_size_dropdown: Entity<DropdownState<Vec<SharedString>>>,
    _tab_size_subscription: gpui::Subscription,
    settings_changed: bool,
    rendered_tabs: std::collections::HashSet<usize>, // Track which tabs have been rendered
    tabs_pending_update: std::collections::HashSet<usize>, // Track tabs that need settings update on next render
}

impl Lightspeed {
    // Create a new Lightspeed instance
    // @param window: The window to create the Lightspeed instance in
    // @param cx: The application context
    // @return: The new Lightspeed instance
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let title_bar = CustomTitleBar::new(window, cx);

        // Create settings
        let settings = match Settings::load() {
            Ok(settings) => settings,
            Err(_) => Settings::new(),
        };

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

            // Create settings dropdown states (logic is in settings.rs)
            let font_size_dropdown = settings::create_font_size_dropdown(&settings, window, cx);
            let tab_size_dropdown = settings::create_tab_size_dropdown(&settings, window, cx);

            // Subscribe to dropdown subscriptions (handler is in settings.rs)
            let _font_size_subscription =
                settings::subscribe_to_font_size_changes(&font_size_dropdown, cx);
            let _tab_size_subscription =
                settings::subscribe_to_tab_size_changes(&tab_size_dropdown, cx);

            // Don't create initial tab here - load_state() will handle it
            let mut entity = Self {
                focus_handle: cx.focus_handle(),
                title_bar,
                tabs: vec![],
                active_tab_index: None,
                next_tab_id: 0,
                show_search: false,
                search_input,
                replace_input,
                match_case: false,
                match_whole_word: false,
                search_matches: Vec::new(),
                current_match_index: None,
                _search_subscription,
                last_search_query: String::new(),
                settings,
                font_size_dropdown,
                _font_size_subscription,
                tab_size_dropdown,
                _tab_size_subscription,
                settings_changed: false,
                rendered_tabs: std::collections::HashSet::new(),
                tabs_pending_update: std::collections::HashSet::new(),
            };

            // Load saved state if it exists
            entity.load_state(window, cx);

            entity
        })
    }

    // Initialize the Lightspeed instance
    // @param cx: The application context
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
    // Render the Lightspeed instance
    // @param window: The window to render the Lightspeed instance in
    // @param cx: The application context
    // @return: The rendered Lightspeed instance
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

        // Update tabs that were marked for update in the previous render
        if !self.tabs_pending_update.is_empty() {
            for tab_index in self.tabs_pending_update.drain() {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get(tab_index) {
                    editor_tab.update_settings(window, cx, &self.settings.editor_settings);
                }
            }
        }

        // Update all rendered editor tabs when settings change
        if self.settings_changed {
            for tab_index in self.rendered_tabs.iter() {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get(*tab_index) {
                    editor_tab.update_settings(window, cx, &self.settings.editor_settings);
                }
            }
            self.settings_changed = false;
        }

        // Mark the active tab as rendered and schedule update for newly rendered tabs
        if let Some(index) = self.active_tab_index {
            let is_newly_rendered = !self.rendered_tabs.contains(&index);
            self.rendered_tabs.insert(index);

            // For newly rendered tabs, schedule an update for the NEXT render
            if is_newly_rendered {
                self.tabs_pending_update.insert(index);
                cx.notify(); // Trigger another render to apply the update
            }
        }

        // Update modified status of tabs
        self.update_modified_status(cx);

        let active_tab = self
            .active_tab_index
            .and_then(|index| self.tabs.get(index).cloned());

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
                    .on_action(cx.listener(|this, _action: &SettingsTab, window, cx| {
                        this.open_settings(window, cx);
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
                    .on_action(
                        cx.listener(|this, action: &tab_bar::CloseTabAction, window, cx| {
                            this.on_close_tab_action(action, window, cx);
                        }),
                    )
                    .on_action(cx.listener(
                        |this, action: &tab_bar::CloseTabsToLeft, window, cx| {
                            this.on_close_tabs_to_left(action, window, cx);
                        },
                    ))
                    .on_action(cx.listener(
                        |this, action: &tab_bar::CloseTabsToRight, window, cx| {
                            this.on_close_tabs_to_right(action, window, cx);
                        },
                    ))
                    .on_action(cx.listener(
                        |this, action: &tab_bar::CloseAllTabsAction, window, cx| {
                            this.on_close_all_tabs_action(action, window, cx);
                        },
                    ))
                    .child(self.title_bar.clone())
                    .child(self.render_tab_bar(window, cx))
                    .child(self.render_content_area(active_tab, window, cx))
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
    // Render the content area (editor or settings)
    // @param active_tab: The active tab (if any)
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered content area element
    fn render_content_area(
        &self,
        active_tab: Option<Tab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let mut content_div = div().flex_1().p_0().m_0().overflow_hidden();

        if let Some(tab) = active_tab {
            match tab {
                Tab::Editor(editor_tab) => {
                    content_div = content_div.child(
                        TextInput::new(&editor_tab.content)
                            .w_full()
                            .h_full()
                            .appearance(false)
                            .text_size(px(self.settings.editor_settings.font_size)),
                    );
                }
                Tab::Settings(_) => {
                    content_div = content_div.child(self.render_settings(window, cx));
                }
            }
        }

        content_div
    }
}
