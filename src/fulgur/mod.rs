mod components_utils;
mod editor_tab;
mod file_operations;
mod icons;
mod languages;
pub mod logger;
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
    ActiveTheme, Icon, Root, Theme, ThemeRegistry, WindowExt, h_flex,
    highlighter::Language,
    input::{Input, InputEvent, InputState},
    link::Link,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    select::SelectState,
    text::TextView,
    v_flex,
};

use crate::fulgur::{icons::CustomIcon, settings::Themes};

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
    pub language_dropdown: Entity<SelectState<Vec<SharedString>>>,
    settings_changed: bool,
    pub themes: Option<Themes>,
    rendered_tabs: std::collections::HashSet<usize>, // Track which tabs have been rendered
    tabs_pending_update: std::collections::HashSet<usize>, // Track tabs that need settings update on next render
    pub pending_files_from_macos: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>, // Files from macOS "Open with" events
    pub show_markdown_preview: bool,
}

impl Fulgur {
    // Create a new Fulgur instance
    // @param window: The window to create the Fulgur instance in
    // @param cx: The application context
    // @param pending_files_from_macos: Arc to the pending files queue from macOS open events
    // @return: The new Fulgur instance
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        pending_files_from_macos: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>>,
    ) -> Entity<Self> {
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
        let language_dropdown =
            languages::create_all_languages_select_state("Plain".into(), window, cx);
        let themes = match Themes::load() {
            Ok(themes) => Some(themes),
            Err(e) => {
                log::error!("Failed to load themes: {}", e);
                None
            }
        };
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
                language_dropdown,
                settings_changed: false,
                rendered_tabs: std::collections::HashSet::new(),
                tabs_pending_update: std::collections::HashSet::new(),
                pending_files_from_macos,
                themes,
                show_markdown_preview: true,
            };
            entity
        });
        entity.update(cx, |this, cx| {
            this.load_state(window, cx);
        });
        entity
    }

    // Initialize the Fulgur instance
    // @param cx: The application context
    pub fn init(cx: &mut App) {
        languages::init_languages();
        let mut settings = Settings::load().unwrap_or_else(|_| Settings::new());
        let recent_files = settings.get_recent_files();
        themes::init(&settings, cx, move |cx| {
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
            let menus = build_menus(cx, &recent_files);
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
        let files_to_open = if let Ok(mut pending) = self.pending_files_from_macos.try_lock() {
            if pending.is_empty() {
                Vec::new()
            } else {
                log::info!(
                    "Processing {} pending file(s) from macOS open event",
                    pending.len()
                );
                pending.drain(..).collect()
            }
        } else {
            Vec::new()
        };
        for file_path in files_to_open {
            self.handle_open_file_from_cli(window, cx, file_path);
        }

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
        let mut app_content = div()
            .id("app-content")
            .size_full()
            .flex()
            .flex_col()
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
                        log::error!("Failed to save settings: {}", e);
                    }
                }
                cx.refresh_windows();
                let menus = build_menus(cx, &this.settings.recent_files.get_files());
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
            .on_action(
                cx.listener(|this, action: &tab_bar::CloseAllOtherTabs, window, cx| {
                    this.on_close_all_other_tabs_action(action, window, cx);
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
            .on_action(cx.listener(|this, action: &OpenRecentFile, window, cx| {
                this.do_open_file(window, cx, action.0.clone());
            }))
            .on_action(cx.listener(|this, _action: &ClearRecentFiles, window, cx| {
                this.clear_recent_files(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &About, window, cx| {
                this.about(window, cx);
            }))
            .on_action(cx.listener(|_, _action: &GetTheme, _window, _cx| {
                if let Err(e) =
                    open::that("https://github.com/longbridge/gpui-component/tree/main/themes")
                {
                    log::error!("Failed to open browser: {}", e);
                }
            }))
            .on_action(cx.listener(|this, _action: &SelectTheme, window, cx| {
                this.select_theme_sheet(window, cx);
            }));
        app_content = app_content
            .child(self.render_tab_bar(window, cx))
            .child(self.render_content_area(active_tab, window, cx))
            .children(self.render_search_bar(window, cx));
        if let Some(index) = self.active_tab_index {
            if let Some(Tab::Editor(_)) = self.tabs.get(index) {
                app_content = app_content.child(self.render_status_bar(window, cx));
            }
        }
        // Create root layout: TitleBar OUTSIDE of focus-tracked content
        // This is critical for Windows hit-testing to work!
        let root_content = v_flex()
            .size_full()
            .child(self.title_bar.clone())
            .child(app_content);
        let main_div = div()
            .size_full()
            .child(root_content)
            .children(Root::render_sheet_layer(window, cx))
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
    ) -> AnyElement {
        if let Some(tab) = active_tab {
            match tab {
                Tab::Editor(editor_tab) => {
                    let editor_input = Input::new(&editor_tab.content)
                        .bordered(false)
                        .p_0()
                        .h_full()
                        .font_family("Monaco")
                        .text_size(px(self.settings.editor_settings.font_size))
                        .focus_bordered(false);
                    if editor_tab.language == Language::Markdown && self.show_markdown_preview {
                        return v_flex()
                            .w_full()
                            .flex_1()
                            .child(
                                h_resizable("markdown-preview-container")
                                    .child(resizable_panel().child(
                                        div().id("markdown-editor").size_full().child(editor_input),
                                    ))
                                    .child(
                                        resizable_panel().child(
                                            TextView::markdown(
                                                "markdown-preview",
                                                editor_tab.content.read(cx).value().clone(),
                                                window,
                                                cx,
                                            )
                                            .flex_none()
                                            .p_5()
                                            .scrollable(true)
                                            .selectable(true),
                                        ),
                                    ),
                            )
                            .into_any_element();
                    }
                    return v_flex()
                        .w_full()
                        .flex_1()
                        .child(editor_input)
                        .into_any_element();
                }
                Tab::Settings(_) => {
                    return v_flex()
                        .id("settings-tab-scrollable")
                        .w_full()
                        .flex_1()
                        .overflow_y_scrollbar()
                        .child(self.render_settings(window, cx))
                        .into_any_element();
                }
            }
        }
        v_flex().w_full().flex_1().into_any_element()
    }

    // Set the title of the title bar
    // @param title: The title to set (if None, the default title is used)
    // @param cx: The application context
    fn set_title(&self, title: Option<String>, cx: &mut Context<Self>) {
        self.title_bar.update(cx, |this, _cx| {
            this.set_title(title);
        });
    }
    // Show the about dialog
    // @param window: The window context
    // @param cx: The application context
    fn about(&self, window: &mut Window, cx: &mut Context<Self>) {
        window.open_dialog(cx, |modal, _window, _cx| {
            modal
                .alert()
                .keyboard(true)
                .title(div().text_center().child("Fulgur"))
                .child(
                    gpui_component::v_flex()
                        .gap_4()
                        .items_center()
                        .child(img("assets/icon_square.png").w(px(200.0)).h(px(200.0)))
                        .child("Version 0.0.1")
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(Icon::new(CustomIcon::Globe))
                                .child(
                                    Link::new("website-link")
                                        .href("https://fulgur.app")
                                        .child("https://fulgur.app"),
                                ),
                        )
                        .child(
                            h_flex()
                                .gap_2()
                                .items_center()
                                .child(Icon::new(CustomIcon::GitHub))
                                .child(
                                    Link::new("github-link")
                                        .href("https://github.com/PRRPCHT/Fulgur")
                                        .child("https://github.com/PRRPCHT/Fulgur"),
                                ),
                        ),
                )
        });
    }
}
