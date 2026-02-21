pub mod files;
pub mod settings;
pub mod shared_state;
pub mod state_operations;
pub mod state_persistence;
pub mod sync;
mod ui;
pub mod utils;
pub mod window_manager;
use crate::register_action;

use crate::fulgur::{
    editor_tab::EditorTab,
    files::file_watcher::FileWatchState,
    sync::sse::SseState,
    ui::{
        dialogs::about::about, languages::SupportedLanguage,
        notifications::update_notification::make_update_notification,
    },
    utils::crypto_helper,
};
use gpui::*;
use gpui_component::{
    ActiveTheme, Root, WindowExt,
    input::{Input, InputEvent, InputState},
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    text::TextView,
    v_flex,
};
use settings::Settings;
use std::{collections::HashSet, sync::Arc, sync::atomic::AtomicBool};
use tab::Tab;
use ui::{
    bars::search_bar_actions::SearchMatch, bars::titlebar::CustomTitleBar, menus::*, tabs::*,
    themes,
};

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
    pub search_subscription: gpui::Subscription,
}

impl SearchState {
    /// Create a new SearchState
    ///
    /// ### Arguments
    /// - `search_input`: The search input entity
    /// - `replace_input`: The replace input entity
    /// - `search_subscription`: The subscription to search input changes
    ///
    /// ### Returns
    /// `Self`: A new SearchState instance with default values
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
            search_subscription,
        }
    }
}

pub struct Fulgur {
    pub window_id: WindowId,                    // The ID of this window
    focus_handle: FocusHandle,                  // The focus handle for the application
    title_bar: Entity<CustomTitleBar>,          // The title bar of the application
    tabs: Vec<Tab>,                             // The tabs in the application
    active_tab_index: Option<usize>,            // Index of the active tab
    next_tab_id: usize,                         // The next tab ID
    pub search_state: SearchState,              // Search and replace functionality state
    pub jump_to_line_input: Entity<InputState>, // Input for jumping to a line in the editor
    pending_jump: Option<editor_tab::Jump>,     // Pending jump to line action
    jump_to_line_dialog_open: bool, // Flag to indicate that the jump to line dialog is open
    pub settings: Settings,         // The settings for the application (local copy for fast access)
    settings_changed: bool, // Flag to indicate that the settings have been changed and need to be saved
    local_settings_version: u64, // Track the version of settings this window has loaded
    rendered_tabs: HashSet<usize>, // Track which tabs have been rendered
    tabs_pending_update: HashSet<usize>, // Track tabs that need settings update on next render
    pub file_watch_state: FileWatchState, // File watching state for external file change detection
    pub sse_state: SseState, // Server-Sent Events state for sync functionality
    pub pending_notification: Option<(NotificationType, SharedString)>, // Pending notification to display on next render
    cached_window_bounds: Option<state_persistence::SerializedWindowBounds>, // Cached window bounds for cross-window saves
}

impl Fulgur {
    /// Get shared application state
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `&'a shared_state::SharedAppState`: The shared application state
    fn shared_state<'a>(&self, cx: &'a App) -> &'a shared_state::SharedAppState {
        cx.global::<shared_state::SharedAppState>()
    }

    /// Create a new Fulgur instance
    ///
    /// ### Arguments
    /// - `window`: The window to create the Fulgur instance in
    /// - `cx`: The application context
    /// - `window_index`: Index of this window in saved state (0 = first window, etc.). Use usize::MAX for new empty windows.
    ///
    /// ### Returns
    /// - `Entity<Self>`: The new Fulgur instance
    pub fn new(window: &mut Window, cx: &mut App, window_index: usize) -> Entity<Self> {
        let title_bar = CustomTitleBar::new(window, cx);
        let shared = cx.global::<shared_state::SharedAppState>();
        let settings = shared.settings.lock().clone();
        let window_id = WindowId::default();
        let search_input = cx.new(|cx| InputState::new(window, cx).placeholder("Search"));
        let replace_input = cx.new(|cx| InputState::new(window, cx).placeholder("Replace"));
        let jump_to_line_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Jump to line or line:character"));
        let entity = cx.new(|cx| {
            let search_subscription =
                cx.subscribe(&search_input, |this: &mut Self, _, ev: &InputEvent, cx| {
                    if let InputEvent::Change = ev
                        && this.search_state.show_search
                    {
                        cx.notify();
                    }
                });
            Self {
                window_id,
                focus_handle: cx.focus_handle(),
                title_bar,
                tabs: vec![],
                active_tab_index: None,
                next_tab_id: 0,
                search_state: SearchState::new(search_input, replace_input, search_subscription),
                jump_to_line_input,
                pending_jump: None,
                jump_to_line_dialog_open: false,
                settings,
                settings_changed: false,
                local_settings_version: 0,
                rendered_tabs: HashSet::new(),
                tabs_pending_update: HashSet::new(),
                file_watch_state: FileWatchState::new(),
                sse_state: SseState::new(),
                pending_notification: None,
                cached_window_bounds: None,
            }
        });
        let (sse_tx, sse_rx) = std::sync::mpsc::channel();
        let sse_shutdown_flag = Arc::new(AtomicBool::new(false));
        entity.update(cx, |this, cx| {
            this.sse_state.sse_events = Some(sse_rx);
            this.sse_state.sse_event_tx = Some(sse_tx);
            this.sse_state.sse_shutdown_flag = Some(sse_shutdown_flag);
            let shared = cx.global::<shared_state::SharedAppState>();
            if let Some(error_msg) = shared.sync_error.lock().as_ref() {
                this.pending_notification =
                    Some((NotificationType::Error, error_msg.clone().into()));
            }
            if window_index == usize::MAX {
                let initial_tab = Tab::Editor(EditorTab::new(
                    0,
                    crate::fulgur::ui::components_utils::UNTITLED,
                    window,
                    cx,
                    &this.settings.editor_settings,
                ));
                this.tabs.push(initial_tab);
                this.active_tab_index = Some(0);
                this.next_tab_id = 1;
            } else {
                this.load_state(window, cx, window_index);
            }
            if this.settings.editor_settings.watch_files {
                this.start_file_watcher();
            }
        });
        sync::synchronization::begin_synchronization(&entity, cx);
        entity
    }

    /// Initialize the Fulgur instance
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn init(cx: &mut App) {
        let mut settings = Settings::load().unwrap_or_else(|e| {
            log::error!("Failed to load settings, using defaults: {}", e);
            Settings::new()
        });
        let recent_files = settings.get_recent_files();
        ui::languages::register_external_languages();
        themes::init(&settings, cx, move |cx| {
            cx.bind_keys([
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-o", OpenFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-o", OpenFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-n", NewFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-o", OpenPath, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-o", OpenPath, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-n", NewFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-n", NewWindow, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-n", NewWindow, None),
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
                KeyBinding::new("alt-f4", Quit, None),
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
            let menus = build_menus(&recent_files, None);
            cx.set_menus(menus);
        });
    }

    /// Process update notifications from the background update checker
    ///
    /// ### Arguments
    /// - `window`: The window to display the notification in
    /// - `cx`: The application context
    fn process_update_notifications(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let update_info = {
            let shared = self.shared_state(cx);
            shared.update_info.lock().take()
        };
        if let Some(update_info) = update_info {
            let notification = make_update_notification(&update_info);
            window.push_notification(notification, cx);
        }
    }

    /// Build the main application content with all action handlers
    ///
    /// ### Arguments
    /// - `active_tab`: The currently active tab (if any) to render in the content area
    /// - `window`: The window to build the content for
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The fully constructed content area with all action handlers attached
    fn build_app_content_with_actions(
        &self,
        active_tab: Option<Tab>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement + use<> {
        let mut app_content = div()
            .id("app-content")
            .size_full()
            .flex()
            .flex_col()
            .gap_0()
            .track_focus(&self.focus_handle);
        register_action!(app_content, cx, NewFile => new_tab);
        register_action!(app_content, cx, OpenFile => open_file);
        register_action!(app_content, cx, OpenPath => show_open_from_path_dialog);
        register_action!(app_content, cx, CloseAllFiles => close_all_tabs);
        register_action!(app_content, cx, SaveFile => save_file);
        register_action!(app_content, cx, SaveFileAs => save_file_as);
        register_action!(app_content, cx, Quit => quit);
        register_action!(app_content, cx, SettingsTab => open_settings);
        register_action!(app_content, cx, FindInFile => find_in_file);
        register_action!(app_content, cx, NextTab => on_next_tab);
        register_action!(app_content, cx, PreviousTab => on_previous_tab);
        register_action!(app_content, cx, JumpToLine => show_jump_to_line_dialog);
        register_action!(app_content, cx, SelectTheme => select_theme_sheet);
        register_action!(app_content, cx, About => call about);
        register_action!(app_content, cx, SwitchTheme => switch_to_theme(.0, no_window));
        register_action!(app_content, cx, tab_bar::CloseTabAction => on_close_tab_action(&action));
        register_action!(app_content, cx, tab_bar::CloseTabsToLeft => on_close_tabs_to_left(&action));
        register_action!(app_content, cx, tab_bar::CloseTabsToRight => on_close_tabs_to_right(&action));
        register_action!(app_content, cx, tab_bar::CloseAllTabsAction => on_close_all_tabs_action(&action));
        register_action!(app_content, cx, tab_bar::CloseAllOtherTabs => on_close_all_other_tabs_action(&action));
        register_action!(app_content, cx, OpenRecentFile => do_open_file(.0));
        register_action!(app_content, cx, CheckForUpdates => check_for_updates);
        register_action!(app_content, cx, GetTheme => call_no_args tab_bar::open_theme_repository);
        register_action!(app_content, cx, NewWindow => open_new_window(cx_only));
        register_action!(app_content, cx, ClearRecentFiles => clear_recent_files(cx_only));
        register_action!(app_content, cx, CloseFile => close_active_tab);
        app_content = app_content
            .child(self.render_tab_bar(cx))
            .child(self.render_content_area(active_tab, window, cx))
            .children(self.render_markdown_bar(cx))
            .children(self.render_search_bar(cx));
        if let Some(index) = self.active_tab_index
            && let Some(Tab::Editor(_)) = self.tabs.get(index)
        {
            app_content = app_content.child(self.render_status_bar(cx));
        }
        app_content
    }

    /// Assemble the final UI tree with all layers
    ///
    /// ### Arguments
    /// - `app_content`: The main content area (from `build_app_content_with_actions()`)
    /// - `window`: The window to assemble the UI for
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The complete UI tree ready to be rendered
    fn assemble_ui_tree(
        &self,
        app_content: impl IntoElement,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        // Create root layout: TitleBar OUTSIDE of focus-tracked content
        // This is critical for Windows hit-testing to work!
        let root_content = v_flex()
            .size_full()
            .child(self.title_bar.clone())
            .child(app_content);
        div()
            .size_full()
            .child(root_content)
            .children(Root::render_sheet_layer(window, cx))
            .children(Root::render_notification_layer(window, cx))
            .children(Root::render_dialog_layer(window, cx))
    }
}

impl Focusable for Fulgur {
    /// Get the focus handle for the Fulgur instance
    ///
    /// ### Arguments
    /// - `_cx`: The application context
    ///
    /// ### Returns
    /// - `FocusHandle`: The focus handle for the Fulgur instance
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

impl Render for Fulgur {
    /// Render the Fulgur instance
    ///
    /// ### Arguments
    /// - `window`: The window to render the Fulgur instance in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered Fulgur instance
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.process_window_state_updates(window, cx);
        self.process_update_notifications(window, cx);
        self.synchronize_settings_from_other_windows(cx);
        self.process_pending_files_from_macos(window, cx);
        self.process_shared_files_from_sync(window, cx);
        self.process_file_watch_events(window, cx);
        self.process_sse_events(window, cx);
        if self.tabs.is_empty() {
            self.active_tab_index = None;
        }
        self.update_search_if_needed(window, cx);
        self.propagate_settings_to_tabs(window, cx);
        self.track_newly_rendered_tabs(cx);
        self.handle_pending_jump_to_line(window, cx);
        if !self.jump_to_line_dialog_open {
            window.close_dialog(cx);
            self.jump_to_line_dialog_open = true;
        }
        self.update_modified_status(cx);
        let active_tab = self
            .active_tab_index
            .and_then(|index| self.tabs.get(index).cloned());
        let app_content = self.build_app_content_with_actions(active_tab.clone(), window, cx);
        self.assemble_ui_tree(app_content, window, cx)
    }
}

impl Fulgur {
    /// Render the content area (editor or settings)
    ///
    /// ### Arguments
    /// - `active_tab`: The active tab (if any)
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered content area element (wrapped in AnyElement)
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
                    if editor_tab.language == SupportedLanguage::Markdown
                        && editor_tab.show_markdown_preview
                    {
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
                                            .py_0()
                                            .px_2()
                                            .scrollable(true)
                                            .selectable(true)
                                            .bg(cx.theme().muted),
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

    /// Set the title of the title bar
    ///
    /// ### Arguments
    /// - `title`: The title to set (if None, the default title is used)
    /// - `cx`: The application context
    fn set_title(&self, title: Option<String>, cx: &mut Context<Self>) {
        self.title_bar.update(cx, |this, _cx| {
            this.set_title(title);
        });
    }
}
