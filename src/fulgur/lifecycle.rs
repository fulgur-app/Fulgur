use crate::fulgur::{
    Fulgur, editor_tab,
    files::file_watcher::FileWatchState,
    languages,
    settings::Settings,
    shared_state, sync,
    tab::{Tab, TabId},
    ui::{
        bars::color_picker_bar::ColorPickerBarState,
        bars::search_bar::SearchState,
        bars::status_bar::StatusBarCache,
        bars::titlebar::CustomTitleBar,
        menus::{build_default_key_bindings, build_menus},
        themes,
    },
    window_manager,
};
use gpui::{App, AppContext, Context, Entity, ScrollHandle, Window, WindowId};
use gpui_component::input::{InputEvent, InputState};
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
                search_state: SearchState::new(search_input, replace_input, search_subscription),
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
                tab_scroll_handle: ScrollHandle::new(),
                pending_tab_scroll: None,
                file_watch_state: FileWatchState::new(),
                save_failed_once: false,
                share_sheet_state: None,
                cached_window_bounds: None,
                font_select_subscription: None,
                editor_context_menu: None,
                editor_context_menu_subscription: None,
                drag_ghost: None,
                status_bar_cache: StatusBarCache::default(),
                cached_tab_filename_counts: HashMap::new(),
                tab_filename_fp: u64::MAX, // sentinel: differs from fingerprint of any real tab list
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
                this.pending_tab_scroll = this.active_tab_id;
                this.pending_initial_active_tab = this.active_tab_id;
            }
            if this.settings.editor_settings.watch_files {
                this.start_file_watcher(cx);
            }
        });
        sync::synchronization::begin_synchronization(&entity, cx);
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
