pub mod files;
pub mod settings;
pub mod shared_state;
pub mod state_operations;
pub mod state_persistence;
pub mod sync;
mod ui;
pub mod utils;
pub mod window_manager;
use crate::fulgur::{
    editor_tab::EditorTab,
    ui::{
        icons::CustomIcon,
        languages::{self, SupportedLanguage},
        notifications::update_notification::make_update_notification,
    },
    utils::crypto_helper::{self, load_private_key_from_keychain},
};
use files::file_watcher::{FileWatchEvent, FileWatcher};
use gpui::*;
use gpui_component::{
    ActiveTheme, Icon, Root, Theme, ThemeRegistry, WindowExt, h_flex,
    input::{Input, InputEvent, InputState},
    link::Link,
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    text::TextView,
    v_flex,
};
use settings::Settings;
use std::sync::mpsc::Receiver;
use std::{
    collections::{HashMap, HashSet},
    path::PathBuf,
    sync::Arc,
    sync::atomic::AtomicBool,
    time::Instant,
};
use tab::Tab;
use ui::{
    bars::search_bar_actions::SearchMatch, bars::titlebar::CustomTitleBar, menus::*, tabs::*,
    themes,
};

/// File watching state for external file change detection
pub struct FileWatchState {
    pub file_watcher: Option<FileWatcher>,
    pub file_watch_events: Option<Receiver<FileWatchEvent>>,
    pub last_file_events: HashMap<PathBuf, Instant>,
    pub last_file_saves: HashMap<PathBuf, Instant>,
    pub pending_conflicts: HashMap<PathBuf, usize>,
}

impl FileWatchState {
    /// Create a new FileWatchState with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self: a new FileWatchState
    pub fn new() -> Self {
        Self {
            file_watcher: None,
            file_watch_events: None,
            last_file_events: HashMap::new(),
            last_file_saves: HashMap::new(),
            pending_conflicts: HashMap::new(),
        }
    }
}

/// Server-Sent Events state for sync functionality
pub struct SseState {
    pub sse_events: Option<Receiver<sync::sse::SseEvent>>,
    pub sse_event_tx: Option<std::sync::mpsc::Sender<sync::sse::SseEvent>>,
    pub sse_shutdown_flag: Option<Arc<AtomicBool>>,
    pub last_sse_event: Option<Instant>,
}

impl SseState {
    /// Create a new SseState with all fields initialized to None
    ///
    /// ### Returns
    /// `Self`: a new SseState
    pub fn new() -> Self {
        Self {
            sse_events: None,
            sse_event_tx: None,
            sse_shutdown_flag: None,
            last_sse_event: None,
        }
    }
}

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

    /// Update settings and propagate to all windows
    ///
    /// This method should be called whenever settings are changed. It will:
    /// 1. Save settings to disk
    /// 2. Update shared settings in SharedAppState
    /// 3. Increment the shared settings version (so other windows detect the change)
    /// 4. Set settings_changed flag for this window
    /// 5. Force all windows to re-render immediately
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `anyhow::Result<()>`: Result of the operation
    fn update_and_propagate_settings(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        // Save settings to disk
        self.settings.save()?;

        // Update shared settings
        let shared = self.shared_state(cx);
        *shared.settings.lock() = self.settings.clone();

        // Increment the version counter so other windows detect the change
        let new_version = shared
            .settings_version
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        self.local_settings_version = new_version;

        // Mark settings as changed for this window
        self.settings_changed = true;

        log::debug!(
            "Window {:?} updated settings to version {}, notifying other windows",
            self.window_id,
            new_version
        );

        // Force other windows to re-render immediately
        // (Skip the current window to avoid reentrancy issues - it will re-render naturally)
        let current_window_id = self.window_id;
        let window_manager = cx.global::<window_manager::WindowManager>();
        let all_windows = window_manager.get_all_windows();

        // Defer notifications to avoid reentrancy issues
        cx.defer(move |cx| {
            for weak_window in all_windows.iter() {
                if let Some(window_entity) = weak_window.upgrade() {
                    // Skip the current window (already updating)
                    let should_notify = window_entity.read(cx).window_id != current_window_id;
                    if should_notify {
                        window_entity.update(cx, |_, cx| {
                            cx.notify();
                        });
                    }
                }
            }
        });

        Ok(())
    }

    /// Handle window close request
    ///
    /// ### Behavior
    /// - If this is the last window: treat as quit (show confirm dialog if enabled)
    /// - If multiple windows exist: just close this window (after saving state)
    ///
    /// ### Arguments
    /// - `window`: The window being closed
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: Allow window to close
    /// - `false`: Prevent window from closing (e.g., waiting for user confirmation)
    pub fn on_window_close_requested(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        let window_count = cx.global::<window_manager::WindowManager>().window_count();
        if window_count == 1 {
            if self.settings.app_settings.confirm_exit {
                self.quit(window, cx);
                false
            } else {
                if let Err(e) = self.save_state(cx, window) {
                    log::error!("Failed to save app state: {}", e);
                }
                cx.update_global::<window_manager::WindowManager, _>(|manager, _| {
                    manager.unregister(self.window_id);
                });
                true
            }
        } else {
            log::debug!(
                "Closing window {:?} ({} windows remaining)",
                self.window_id,
                window_count - 1
            );
            cx.update_global::<window_manager::WindowManager, _>(|manager, _| {
                manager.unregister(self.window_id);
            });
            if let Err(e) = self.save_state(cx, window) {
                log::error!("Failed to save app state: {}", e);
            }
            true
        }
    }

    /// Open a new Fulgur window (completely empty)
    ///
    /// ### Arguments
    /// - `cx` - The context for the application
    pub fn open_new_window(&self, cx: &mut Context<Self>) {
        let async_cx = cx.to_async();
        async_cx
            .spawn(async move |cx| {
                let window_options = WindowOptions {
                    titlebar: Some(gpui_component::TitleBar::title_bar_options()),
                    #[cfg(target_os = "linux")]
                    window_decorations: Some(gpui::WindowDecorations::Client),
                    ..Default::default()
                };
                let window = cx.open_window(window_options, |window, cx| {
                    window.set_window_title("Fulgur");
                    let view = Fulgur::new(window, cx, usize::MAX); // usize::MAX = new empty window
                    let window_handle = window.window_handle();
                    let window_id = window_handle.window_id();
                    view.update(cx, |fulgur, _cx| {
                        fulgur.window_id = window_id;
                    });
                    cx.update_global::<window_manager::WindowManager, _>(|manager, _| {
                        manager.register(window_id, view.downgrade());
                    });
                    let view_clone = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        view_clone.update(cx, |fulgur, cx| {
                            fulgur.on_window_close_requested(window, cx)
                        })
                    });
                    view.read(cx).focus_active_tab(window, cx);
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                })?;
                window.update(cx, |_, window, _| {
                    window.activate_window();
                })?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
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
        languages::init_languages();
        let mut settings = Settings::load().unwrap_or_else(|e| {
            log::error!("Failed to load settings, using defaults: {}", e);
            Settings::new()
        });
        let recent_files = settings.get_recent_files();
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

    /// Process window state updates during the render cycle:
    /// 1. Cache the current window bounds and display ID for state persistence
    /// 2. Update the global WindowManager to track this window as focused
    /// 3. Display any pending notifications that were queued during event processing
    ///
    /// ### Arguments
    /// - `window`: The window being rendered
    /// - `cx`: The application context
    fn process_window_state_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let display_id = window.display(cx).map(|d| d.id().into());
        self.cached_window_bounds =
            Some(state_persistence::SerializedWindowBounds::from_gpui_bounds(
                window.window_bounds(),
                display_id,
            ));
        cx.update_global::<window_manager::WindowManager, _>(|manager, _| {
            manager.set_focused(self.window_id);
        });
        if let Some((notification_type, message)) = self.pending_notification.take() {
            window.push_notification((notification_type, message), cx);
        }
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

    /// Synchronize settings from other windows
    ///
    /// ### Arguments
    /// - `cx`: The application context
    fn synchronize_settings_from_other_windows(&mut self, cx: &mut Context<Self>) {
        let shared = self.shared_state(cx);
        let shared_version = shared
            .settings_version
            .load(std::sync::atomic::Ordering::Relaxed);
        if shared_version > self.local_settings_version {
            // Settings have been updated in another window - reload them
            let shared_settings = shared.settings.lock().clone();
            self.settings = shared_settings;
            self.local_settings_version = shared_version;
            self.settings_changed = true;
            log::debug!(
                "Window {:?} detected settings change from another window (version {} -> {})",
                self.window_id,
                self.local_settings_version,
                shared_version
            );
        }
    }

    /// Process pending files from macOS "Open With" events
    ///
    /// ### Arguments
    /// - `window`: The window to open files in
    /// - `cx`: The application context
    fn process_pending_files_from_macos(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shared = self.shared_state(cx);
        let should_process_files = cx
            .global::<window_manager::WindowManager>()
            .get_last_focused()
            .map(|id| id == self.window_id)
            .unwrap_or(true); // If no last focused window, allow this one to process
        let files_to_open = if should_process_files {
            if let Some(mut pending) = shared.pending_files_from_macos.try_lock() {
                if pending.is_empty() {
                    Vec::new()
                } else {
                    log::info!(
                        "Processing {} pending file(s) from macOS open event in window {:?}",
                        pending.len(),
                        self.window_id
                    );
                    pending.drain(..).collect()
                }
            } else {
                Vec::new()
            }
        } else {
            Vec::new()
        };
        for file_path in files_to_open {
            self.handle_open_file_from_cli(window, cx, file_path);
        }
    }

    /// Process shared files from the sync server
    ///
    /// ### Arguments
    /// - `window`: The window to create new tabs in
    /// - `cx`: The application context
    fn process_shared_files_from_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shared_files_to_open =
            if let Some(mut pending) = self.shared_state(cx).pending_shared_files.try_lock() {
                if pending.is_empty() {
                    Vec::new()
                } else {
                    log::info!(
                        "Processing {} shared file(s) from sync server",
                        pending.len()
                    );
                    pending.drain(..).collect()
                }
            } else {
                Vec::new()
            };
        if !shared_files_to_open.is_empty() {
            let encryption_key_opt = match load_private_key_from_keychain() {
                Ok(key) => key,
                Err(_) => {
                    log::error!("Cannot decrypt shared files: encryption key not available");
                    None
                }
            };
            if let Some(encryption_key) = encryption_key_opt {
                for shared_file in shared_files_to_open {
                    let decrypted_result =
                        crypto_helper::decrypt_bytes(&shared_file.content, &encryption_key)
                            .and_then(|compressed_bytes| {
                                sync::share::decompress_content(&compressed_bytes)
                            });
                    match decrypted_result {
                        Ok(decrypted_content) => {
                            let tab_id = self.next_tab_id;
                            self.next_tab_id += 1;
                            let new_tab = Tab::Editor(editor_tab::EditorTab::from_content(
                                tab_id,
                                decrypted_content,
                                shared_file.file_name.clone(),
                                window,
                                cx,
                                &self.settings.editor_settings,
                            ));
                            self.tabs.push(new_tab);
                            self.active_tab_index = Some(self.tabs.len() - 1);
                            log::info!("Opened shared file: {}", shared_file.file_name);
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to decrypt shared file {}: {}",
                                shared_file.file_name,
                                e
                            );
                        }
                    }
                }
            } else {
                log::error!("Cannot decrypt shared files: encryption key not available");
            }
        }
    }

    /// Collect and process file watch events:
    /// - Modified: File content changed externally (may trigger auto-reload or conflict dialog)
    /// - Deleted: File was deleted externally (shows notification)
    /// - Renamed: File was moved/renamed (updates tab path and continues watching)
    ///
    /// ### Arguments
    /// - `window`: The window containing the tabs with watched files
    /// - `cx`: The application context
    fn process_file_watch_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let events: Vec<FileWatchEvent> =
            if let Some(ref rx) = self.file_watch_state.file_watch_events {
                let mut events = Vec::new();
                while let Ok(event) = rx.try_recv() {
                    events.push(event);
                }
                events
            } else {
                Vec::new()
            };
        for event in events {
            self.handle_file_watch_event(event, window, cx);
        }
    }

    /// Collect and process Server-Sent Events from the sync server:
    /// - Heartbeat: Periodic keepalive messages to detect connection timeouts
    /// - ShareAvailable: Another device has shared a file (triggers file download and decryption)
    /// - Error: Connection or server errors (updates connection status in UI)
    ///
    /// ### Arguments
    /// - `window`: The window to handle events in
    /// - `cx`: The application context
    fn process_sse_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sse_events: Vec<sync::sse::SseEvent> = if let Some(ref rx) = self.sse_state.sse_events {
            let mut events = Vec::new();
            while let Ok(event) = rx.try_recv() {
                events.push(event);
            }
            events
        } else {
            Vec::new()
        };
        for event in sse_events {
            self.handle_sse_event(event, window, cx);
        }
    }

    /// Update search results if the search query has changed
    ///
    /// ### Arguments
    /// - `window`: The window containing the search bar and editor
    /// - `cx`: The application context
    fn update_search_if_needed(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.search_state.show_search {
            let current_query = self.search_state.search_input.read(cx).text().to_string();
            if current_query != self.search_state.last_search_query {
                self.search_state.last_search_query = current_query;
                self.perform_search(window, cx);
            }
        }
    }

    /// Propagate settings changes to tabs
    ///
    /// ### Arguments
    /// - `window`: The window containing the tabs
    /// - `cx`: The application context
    fn propagate_settings_to_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs_pending_update.is_empty() {
            let settings = self.settings.editor_settings.clone();
            for tab_index in self.tabs_pending_update.drain() {
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
    }

    /// Track newly rendered tabs and mark them for settings update
    ///
    /// ### Arguments
    /// - `cx`: The application context
    fn track_newly_rendered_tabs(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.active_tab_index {
            let is_newly_rendered = !self.rendered_tabs.contains(&index);
            self.rendered_tabs.insert(index);
            if is_newly_rendered {
                self.tabs_pending_update.insert(index);
                cx.notify();
            }
        }
    }

    /// Handle pending jump-to-line action
    ///
    /// ### Arguments
    /// - `window`: The window containing the editor
    /// - `cx`: The application context
    fn handle_pending_jump_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(jump) = self.pending_jump.take()
            && let Some(index) = self.active_tab_index
            && let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(index)
        {
            editor_tab.jump_to_line(window, cx, jump);
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
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _action: &NewFile, window, cx| {
                this.new_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &NewWindow, _window, cx| {
                this.open_new_window(cx);
            }))
            .on_action(cx.listener(|this, _action: &OpenFile, window, cx| {
                this.open_file(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &OpenPath, window, cx| {
                this.show_open_from_path_dialog(window, cx);
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
                    if let Err(e) = this.update_and_propagate_settings(cx) {
                        log::error!("Failed to save settings: {}", e);
                    }
                }
                cx.refresh_windows();
                let menus = build_menus(this.settings.recent_files.get_files(), None);
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
                this.show_jump_to_line_dialog(window, cx);
            }))
            .on_action(cx.listener(|this, action: &OpenRecentFile, window, cx| {
                this.do_open_file(window, cx, action.0.clone());
            }))
            .on_action(
                cx.listener(|this, _action: &ClearRecentFiles, _window, cx| {
                    this.clear_recent_files(cx);
                }),
            )
            .on_action(cx.listener(|_this, _action: &About, window, cx| {
                about(window, cx);
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
            }))
            .on_action(cx.listener(|this, _action: &CheckForUpdates, window, cx| {
                this.check_for_updates(window, cx);
            }));
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

/// Show the about dialog
///
/// ### Arguments
/// - `window`: The window context
/// - `cx`: The application context
fn about(window: &mut Window, cx: &mut App) {
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
                    .child(format!("Version {}", env!("CARGO_PKG_VERSION")))
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
                                    .href("https://github.com/fulgur-app/Fulgur")
                                    .child("https://github.com/fulgur-app/Fulgur"),
                            ),
                    )
                    .child(
                        h_flex()
                            .gap_2()
                            .items_center()
                            .child(Icon::new(CustomIcon::File))
                            .child(
                                Link::new("license-link")
                                    .href("http://www.apache.org/licenses/LICENSE-2.0")
                                    .child("http://www.apache.org/licenses/LICENSE-2.0"),
                            ),
                    ),
            )
    });
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
