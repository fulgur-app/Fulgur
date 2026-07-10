mod content_area;
pub mod files;
pub mod languages;
mod lifecycle;
mod render;
pub mod settings;
pub mod shared_state;
pub mod state;
pub mod sync;
mod ui;
pub mod utils;
pub mod window_manager;

use crate::fulgur::files::{
    file_operations::PendingRemoteOpenOutcome, file_watcher::FileWatchState,
};
use gpui::{Entity, EntityId, FocusHandle, Pixels, Point, SharedString, Subscription, WindowId};
use gpui_component::{input::InputState, menu::PopupMenu};
use settings::Settings;
use std::{collections::HashMap, collections::HashSet, sync::Arc, sync::atomic::AtomicBool};
use tab::{Tab, TabId};
use ui::log_view::LogTailState;
use ui::{
    bars::color_picker_bar::ColorPickerBarState,
    bars::markdown_toolbar::MarkdownToolbar,
    bars::search_bar::SearchBar,
    bars::status_bar::StatusBar,
    bars::titlebar::CustomTitleBar,
    tabs::{editor_tab, tab, tab_bar::TabBar},
};

// Re-export so descendant modules can keep using `crate::fulgur::themes::...`.
pub(crate) use ui::themes;

pub struct Fulgur {
    pub window_id: WindowId,                         // The ID of this window
    focus_handle: FocusHandle,                       // The focus handle for the application
    title_bar: Entity<CustomTitleBar>,               // The title bar of the application
    tabs: Vec<Tab>,                                  // The tabs in the application
    active_tab_id: Option<TabId>,                    // Stable ID of the active tab
    next_tab_id: TabId,                              // The next tab ID
    search_bar: Entity<SearchBar>,                   // The search and replace bar view
    _search_bar_subscription: Subscription, // Routes SearchBarEvent from the search bar to window-level handlers
    markdown_toolbar: Entity<MarkdownToolbar>, // The markdown formatting toolbar view (acts directly on the active editor, emits no events)
    pub color_picker_bar_state: ColorPickerBarState, // Color picker bar state
    pub jump_to_line_input: Entity<InputState>, // Input for jumping to a line in the editor
    pending_jump: Option<editor_tab::Jump>,    // Pending jump to line action
    pub settings: Settings, // The settings for the application (local snapshot, refreshed by the SharedAppState observer)
    settings_changed: bool, // Flag to indicate that the settings have been changed and need to be saved
    _shared_state_observation: Subscription, // Global observer keeping the local settings snapshot in sync with SharedAppState
    rendered_tabs: HashSet<TabId>,           // Track which tabs have been rendered
    tabs_pending_update: HashSet<TabId>,     // Track tabs that need settings update on next render
    editor_modified_subscriptions: HashMap<TabId, (EntityId, Subscription)>, // Per-editor (subscribed content entity id, subscription) for incremental modified-state updates
    markdown_preview_cache: HashMap<TabId, SharedString>, // Cached markdown source text keyed by source editor tab id
    markdown_preview_to_refresh: HashSet<TabId>, // Source tab ids whose cached preview text is stale and must be refreshed on next read
    markdown_preview_subscriptions: HashMap<TabId, (EntityId, Subscription)>, // Per-source (subscribed content entity id, subscription) for markdown preview cache updates
    log_tail_state: HashMap<TabId, LogTailState>, // Per-log-tab tail bookkeeping (byte offset, dropped lines, pending text) keyed by tab id
    log_tail_cancel: HashMap<TabId, Arc<AtomicBool>>, // Cancellation flag for the per-active-log-tab poll task keyed by tab id
    pub file_watch_state: FileWatchState, // File watching state for external file change detection
    save_failed_once: bool, // Flag: save already failed once, allow force-close on next attempt
    pub share_sheet_state: Option<Arc<ui::sheets::share_file::ShareSheetState>>, // When Some, a share sheet is open and devices are being fetched per profile
    cached_window_bounds: Option<state::SerializedWindowBounds>, // Cached window bounds for cross-window saves
    font_select_subscription: Option<Subscription>, // Subscription for font family selection events (set when settings tab is opened)
    editor_context_menu: Option<(Point<Pixels>, Entity<PopupMenu>)>, // Custom right-click context menu for the editor
    editor_context_menu_subscription: Option<Subscription>, // Subscription to clear editor_context_menu on dismiss
    status_bar: Entity<StatusBar>, // The status bar view at the bottom of the window
    _status_bar_subscription: Subscription, // Routes StatusBarEvent from the status bar to window-level handlers
    tab_bar: Entity<TabBar>,                // The tab bar view at the top of the window
    _tab_bar_subscription: Subscription, // Routes TabBarEvent from the tab bar to window-level handlers
    pub pending_tab_transfer: Option<editor_tab::TabTransferData>, // Incoming tab state from another window, processed on next render
    pending_tab_removal: Option<TabId>, // Tab ID to remove after it has been sent to another window
    pending_transfer_scroll: Option<gpui_component::input::Position>, // Deferred scroll-to-cursor after tab transfer (needs one render cycle for layout)
    pending_remote_open: Arc<parking_lot::Mutex<Vec<PendingRemoteOpenOutcome>>>, // Queue for SSH background threads to deliver loaded remote files
    next_remote_request_id: u64, // Monotonic identifier for remote open/save operations targeting existing tabs
    latest_remote_open_request_by_tab: HashMap<TabId, u64>, // Latest remote-open request id expected per tab id
    latest_remote_save_request_by_tab: HashMap<TabId, u64>, // Latest remote-save request id expected per tab id
    last_failed_remote_open_url: Option<String>, // Last attempted remote URL kept for retry prefill after connection/open failures
    pending_remote_restore: HashSet<TabId>, // Restored remote tab ids that should lazily reconnect on first activation/save
    inflight_remote_restore: HashSet<TabId>, // Restored remote tabs currently running a reconnect task
    pending_initial_active_tab: Option<TabId>, // Active tab to re-activate after first render so dialogs can open safely
    has_rendered_once: bool, // Tracks first render completion for startup actions that require mounted Root layers
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    local_window_menu_fingerprint: u64, // Cached local menu-state fingerprint published to WindowManager
    #[cfg(target_os = "macos")]
    last_dock_menu_revision: u64, // Last global menu-state revision processed by dock menu updater
    #[cfg(target_os = "macos")]
    last_dock_menu_hash: u64, // Hash of the last dock menu state to avoid unnecessary rebuilds
    #[cfg(target_os = "windows")]
    last_jump_list_revision: u64, // Last global menu-state revision processed by jump list updater
    #[cfg(target_os = "windows")]
    last_jump_list_hash: u64, // Hash of the last jump list state to avoid unnecessary rebuilds
}
