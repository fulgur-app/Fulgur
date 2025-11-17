mod components_utils;
mod editor_tab;
mod file_operations;
mod icons;
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
    ActiveTheme, Root, Theme, ThemeRegistry, WindowExt,
    input::{Input, InputEvent, InputState},
    select::SelectState,
    v_flex,
};

pub struct Fulgur {
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
    pub jump_to_line_input: Entity<InputState>,
    pending_jump: Option<editor_tab::Jump>,
    jump_to_line_dialog_open: bool,
    pub settings: Settings,
    pub font_size_dropdown: Entity<SelectState<Vec<SharedString>>>,
    _font_size_subscription: gpui::Subscription,
    pub tab_size_dropdown: Entity<SelectState<Vec<SharedString>>>,
    _tab_size_subscription: gpui::Subscription,
    settings_changed: bool,
    rendered_tabs: std::collections::HashSet<usize>, // Track which tabs have been rendered
    tabs_pending_update: std::collections::HashSet<usize>, // Track tabs that need settings update on next render
}

impl Fulgur {
    // Create a new Fulgur instance
    // @param window: The window to create the Fulgur instance in
    // @param cx: The application context
    // @return: The new Fulgur instance
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let title_bar = CustomTitleBar::new(window, cx);
        let settings = match Settings::load() {
            Ok(settings) => settings,
            Err(_) => Settings::new(),
        };
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
        let jump_to_line_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Jump to line or line:character"));
        let font_size_dropdown = settings::create_font_size_dropdown(&settings, window, cx);
        let tab_size_dropdown = settings::create_tab_size_dropdown(&settings, window, cx);
        let entity = cx.new(|cx| {
            let _search_subscription = cx.subscribe(
                &search_input,
                |this: &mut Self, _, ev: &InputEvent, cx| match ev {
                    InputEvent::Change => {
                        if this.show_search {
                            cx.notify();
                        }
                    }
                    _ => {}
                },
            );
            let _font_size_subscription =
                settings::subscribe_to_font_size_changes(&font_size_dropdown, cx);
            let _tab_size_subscription =
                settings::subscribe_to_tab_size_changes(&tab_size_dropdown, cx);
            let entity = Self {
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
                jump_to_line_input,
                pending_jump: None,
                jump_to_line_dialog_open: false,
                settings,
                font_size_dropdown,
                _font_size_subscription,
                tab_size_dropdown,
                _tab_size_subscription,
                settings_changed: false,
                rendered_tabs: std::collections::HashSet::new(),
                tabs_pending_update: std::collections::HashSet::new(),
            };
            entity
        });
        // Load state after entity creation
        entity.update(cx, |this, cx| {
            this.load_state(window, cx);
        });
        entity
    }

    // Initialize the Fulgur instance
    // @param cx: The application context
    pub fn init(cx: &mut App) {
        languages::init_languages();
        let settings = Settings::load().unwrap_or_else(|_| Settings::new());
        themes::init(&settings, cx, |cx| {
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
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-right", NextTab, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-right", NextTab, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-left", PreviousTab, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-left", PreviousTab, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("ctrl-g", JumpToLine, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-g", JumpToLine, None),
            ]);
            let menus = build_menus(cx);
            cx.set_menus(menus);
        });
    }
}

impl Focusable for Fulgur {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Fulgur {
    // Render the Fulgur instance
    // @param window: The window to render the Fulgur instance in
    // @param cx: The application context
    // @return: The rendered Fulgur instance
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        }
        if self.show_search {
            let current_query = self.search_input.read(cx).text().to_string();
            if current_query != self.last_search_query {
                self.last_search_query = current_query;
                self.perform_search(window, cx);
            }
        }
        if !self.tabs_pending_update.is_empty() {
            let settings = self.settings.editor_settings.clone();
            for tab_index in self.tabs_pending_update.drain().collect::<Vec<_>>() {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
                    editor_tab.update_settings(window, cx, &settings);
                }
            }
        }
        if self.settings_changed {
            let settings = self.settings.editor_settings.clone();
            for tab_index in self.rendered_tabs.iter().copied().collect::<Vec<_>>() {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
                    editor_tab.update_settings(window, cx, &settings);
                }
            }
            self.settings_changed = false;
        }
        if let Some(index) = self.active_tab_index {
            let is_newly_rendered = !self.rendered_tabs.contains(&index);
            self.rendered_tabs.insert(index);
            if is_newly_rendered {
                self.tabs_pending_update.insert(index);
                cx.notify();
            }
        }
        if let Some(jump) = self.pending_jump.take() {
            if let Some(index) = self.active_tab_index {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(index) {
                    editor_tab.jump_to_line(window, cx, jump);
                }
            }
        }
        if !self.jump_to_line_dialog_open {
            window.close_dialog(cx);
            self.jump_to_line_dialog_open = true;
        }
        self.update_modified_status(cx);
        let active_tab = self
            .active_tab_index
            .and_then(|index| self.tabs.get(index).cloned());
        let mut content = v_flex()
            .size_full()
            .gap_0()
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
                this.find_in_file(window, cx);
            }))
            .on_action(cx.listener(|this, action: &SwitchTheme, _window, cx| {
                let theme_name = action.0.clone();
                if let Some(theme_config) =
                    ThemeRegistry::global(cx).themes().get(&theme_name).cloned()
                {
                    Theme::global_mut(cx).apply_config(&theme_config);
                    this.settings.app_settings.theme = theme_name;
                    this.settings.app_settings.scrollbar_show = Some(cx.theme().scrollbar_show);
                    if let Err(e) = this.settings.save() {
                        eprintln!("Failed to save settings: {}", e);
                    }
                }
                cx.refresh_windows();
                let menus = build_menus(cx);
                cx.set_menus(menus);
            }))
            .on_action(
                cx.listener(|this, action: &tab_bar::CloseTabAction, window, cx| {
                    this.on_close_tab_action(action, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &tab_bar::CloseTabsToLeft, window, cx| {
                    this.on_close_tabs_to_left(action, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &tab_bar::CloseTabsToRight, window, cx| {
                    this.on_close_tabs_to_right(action, window, cx);
                }),
            )
            .on_action(
                cx.listener(|this, action: &tab_bar::CloseAllTabsAction, window, cx| {
                    this.on_close_all_tabs_action(action, window, cx);
                }),
            )
            .on_action(cx.listener(|this, _action: &NextTab, window, cx| {
                this.on_next_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &PreviousTab, window, cx| {
                this.on_previous_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &JumpToLine, window, cx| {
                this.jump_to_line(window, cx);
            }))
            //.child(self.title_bar.clone())
            .child(self.render_tab_bar(window, cx))
            .child(self.render_content_area(active_tab, window, cx))
            .children(self.render_search_bar(window, cx));
        if let Some(index) = self.active_tab_index {
            if let Some(Tab::Editor(_)) = self.tabs.get(index) {
                content = content.child(self.render_status_bar(window, cx));
            }
        }
        let main_div = div()
            .size_full()
            .child(content)
            .children(Root::render_notification_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx));
        main_div
    }
}

impl Fulgur {
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
    ) -> impl IntoElement {
        if let Some(tab) = active_tab {
            match tab {
                Tab::Editor(editor_tab) => {
                    return v_flex().w_full().flex_1().child(
                        Input::new(&editor_tab.content)
                            .bordered(false)
                            .p_0()
                            .h_full()
                            .font_family("Monaco")
                            .text_size(px(self.settings.editor_settings.font_size))
                            .focus_bordered(false),
                    );
                }
                Tab::Settings(_) => {
                    return v_flex()
                        .w_full()
                        .flex_1()
                        .child(self.render_settings(window, cx));
                }
            }
        }
        v_flex().w_full().flex_1()
    }
}
