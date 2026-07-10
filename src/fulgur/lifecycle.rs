use crate::fulgur::{
    Fulgur, editor_tab,
    files::file_watcher::FileWatchState,
    languages,
    settings::Settings,
    shared_state,
    tab::{Tab, TabId},
    ui::{
        bars::color_picker_bar::ColorPickerBarState,
        bars::markdown_toolbar::MarkdownToolbar,
        bars::search_bar::{SearchBar, SearchBarEvent},
        bars::status_bar::{StatusBar, StatusBarEvent},
        bars::titlebar::CustomTitleBar,
        menus::{build_default_key_bindings, build_menus},
        tabs::tab_bar::{TabBar, TabBarEvent},
        themes,
    },
    window_manager,
};
use gpui::{App, AppContext, Context, Entity, Window, WindowId};
use gpui_component::input::InputState;
use gpui_component::notification::NotificationType;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

impl Fulgur {
    /// Get shared application state
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `&'a shared_state::SharedAppState`: The shared application state
    pub(super) fn shared_state(cx: &App) -> &shared_state::SharedAppState {
        cx.global::<shared_state::SharedAppState>()
    }

    /// Create a new Fulgur instance
    ///
    /// ### Arguments
    /// - `window`: The window to create the Fulgur instance in
    /// - `cx`: The application context
    /// - `window_id`: The window ID for this instance, obtained from `window.window_handle().window_id()`
    /// - `window_index`: Index of this window in saved state (0 = first window, etc.). Use `usize::MAX` for new empty windows.
    ///
    /// ### Returns
    /// - `Entity<Self>`: The new Fulgur instance
    pub fn new(
        window: &mut Window,
        cx: &mut App,
        window_id: WindowId,
        window_index: usize,
    ) -> Entity<Self> {
        let title_bar = CustomTitleBar::new(window, cx);
        let shared = cx.global::<shared_state::SharedAppState>();
        let settings = shared.settings.clone();
        let jump_to_line_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Jump to line or line:character"));
        let entity = cx.new(|cx| {
            let weak_fulgur = cx.weak_entity();
            let search_bar = cx.new(|cx| SearchBar::new(weak_fulgur.clone(), window, cx));
            let search_bar_subscription = cx.subscribe_in(
                &search_bar,
                window,
                |this: &mut Self, _, event: &SearchBarEvent, window, cx| {
                    this.on_search_bar_event(*event, window, cx);
                },
            );

            let status_bar = cx.new(|_| StatusBar::new(weak_fulgur.clone()));
            let status_bar_subscription = cx.subscribe_in(
                &status_bar,
                window,
                |this: &mut Self, _, event: &StatusBarEvent, window, cx| {
                    this.on_status_bar_event(*event, window, cx);
                },
            );

            let markdown_toolbar = cx.new(|_| MarkdownToolbar::new(weak_fulgur.clone()));

            let tab_bar = cx.new(|_| TabBar::new(weak_fulgur));
            let tab_bar_subscription = cx.subscribe_in(
                &tab_bar,
                window,
                |this: &mut Self, _, event: &TabBarEvent, window, cx| {
                    this.on_tab_bar_event(event, window, cx);
                },
            );

            let shared_state_observation =
                cx.observe_global::<shared_state::SharedAppState>(|this: &mut Self, cx| {
                    let shared = cx.global::<shared_state::SharedAppState>();
                    if this.settings != shared.settings {
                        this.settings = shared.settings.clone();
                        this.settings_changed = true;
                    }
                    cx.notify();
                });
            Self {
                window_id,
                focus_handle: cx.focus_handle(),
                title_bar,
                tabs: vec![],
                active_tab_id: None,
                next_tab_id: TabId(0),
                search_bar,
                _search_bar_subscription: search_bar_subscription,
                markdown_toolbar,
                color_picker_bar_state: ColorPickerBarState::new(window, cx),
                jump_to_line_input,
                pending_jump: None,
                settings,
                settings_changed: false,
                _shared_state_observation: shared_state_observation,
                rendered_tabs: HashSet::new(),
                tabs_pending_update: HashSet::new(),
                editor_modified_subscriptions: HashMap::new(),
                markdown_preview_cache: HashMap::new(),
                markdown_preview_to_refresh: HashSet::new(),
                markdown_preview_subscriptions: HashMap::new(),
                log_tail_state: HashMap::new(),
                log_tail_cancel: HashMap::new(),
                file_watch_state: FileWatchState::new(),
                save_failed_once: false,
                share_sheet_state: None,
                cached_window_bounds: None,
                font_select_subscription: None,
                editor_context_menu: None,
                editor_context_menu_subscription: None,
                status_bar,
                _status_bar_subscription: status_bar_subscription,
                tab_bar,
                _tab_bar_subscription: tab_bar_subscription,
                pending_tab_transfer: None,
                pending_tab_removal: None,
                pending_transfer_scroll: None,
                pending_remote_open: Arc::new(parking_lot::Mutex::new(Vec::new())),
                next_remote_request_id: 1,
                latest_remote_open_request_by_tab: HashMap::new(),
                latest_remote_save_request_by_tab: HashMap::new(),
                last_failed_remote_open_url: None,
                pending_remote_restore: HashSet::new(),
                inflight_remote_restore: HashSet::new(),
                pending_initial_active_tab: None,
                has_rendered_once: false,
                #[cfg(any(target_os = "macos", target_os = "windows"))]
                local_window_menu_fingerprint: 0,
                #[cfg(target_os = "macos")]
                last_dock_menu_revision: 0,
                #[cfg(target_os = "macos")]
                last_dock_menu_hash: 0,
                #[cfg(target_os = "windows")]
                last_jump_list_revision: 0,
                #[cfg(target_os = "windows")]
                last_jump_list_hash: 0,
            }
        });
        entity.update(cx, |this, cx| {
            let shared = cx.global::<shared_state::SharedAppState>();
            if let Some(error_msg) = shared.sync_error.lock().as_ref() {
                shared.notify((NotificationType::Error, error_msg.clone().into()));
            }
            if window_index == usize::MAX {
                let initial_tab = Tab::Editor(editor_tab::EditorTab::new(
                    TabId(0),
                    crate::fulgur::ui::components_utils::UNTITLED,
                    window,
                    cx,
                    &this.settings.editor_settings,
                ));
                this.active_tab_id = Some(initial_tab.id());
                this.tabs.push(initial_tab);
                this.next_tab_id = TabId(1);
            } else if window_index < usize::MAX - 1 {
                // usize::MAX - 1 means new window receiving a tab transfer: skip initial tab
                this.load_state(window, cx, window_index);
                if let Some(tab_id) = this.active_tab_id {
                    this.request_tab_scroll(tab_id, cx);
                }
                this.pending_initial_active_tab = this.active_tab_id;
            }
            if this.settings.editor_settings.watch_files {
                this.start_file_watcher(cx);
            }
        });
        // Skip real sync under `cargo test`.
        #[cfg(not(test))]
        crate::fulgur::sync::synchronization::begin_synchronization(&entity, cx);
        entity
    }

    /// Initialize the Fulgur instance
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `settings`: The application settings (already loaded and resolved, including first-run overrides)
    pub fn init(cx: &mut App, settings: &mut Settings) {
        let recent_files = settings.get_recent_files();
        languages::supported_languages::register_external_languages();
        themes::init(settings, cx, move |cx| {
            cx.bind_keys(build_default_key_bindings());
            let menus = build_menus(&recent_files, None);
            cx.set_menus(menus);
            #[cfg(not(target_os = "macos"))]
            if let Some(owned_menus) = cx.get_menus() {
                gpui_component::GlobalState::global_mut(cx).set_app_menus(owned_menus);
            }
        });
    }

    /// Set the title of the title bar.
    ///
    /// Looks up the current window name from the global `WindowManager` and passes it to
    /// `CustomTitleBar::set_title` so the suffix is automatically included when multiple
    /// windows are open.
    ///
    /// ### Arguments
    /// - `title`: The title to set (if None, the default title is used)
    /// - `cx`: The application context
    pub(super) fn set_title(&self, title: Option<String>, cx: &mut Context<Self>) {
        let window_name = cx
            .global::<window_manager::WindowManager>()
            .get_window_name(self.window_id)
            .map(std::string::ToString::to_string);
        self.title_bar.update(cx, |this, _cx| {
            this.set_title(title, window_name.as_deref());
        });
    }
}
