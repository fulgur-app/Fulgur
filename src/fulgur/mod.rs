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
use gpui::{
    Entity, EntityId, FocusHandle, Pixels, Point, ScrollHandle, SharedString, Subscription,
    WindowId,
};
use gpui_component::{input::InputState, menu::PopupMenu, notification::NotificationType};
use settings::Settings;
use std::{collections::HashMap, collections::HashSet, sync::Arc};
use tab::Tab;
use ui::{
    bars::color_picker_bar::ColorPickerBarState,
    bars::search_bar::SearchState,
    bars::status_bar::StatusBarCache,
    bars::titlebar::CustomTitleBar,
    tabs::{editor_tab, tab},
};

// Re-export so descendant modules can keep using `crate::fulgur::themes::...`.
pub(crate) use ui::themes;

pub struct Fulgur {
    pub window_id: WindowId,                         // The ID of this window
    focus_handle: FocusHandle,                       // The focus handle for the application
    title_bar: Entity<CustomTitleBar>,               // The title bar of the application
    tabs: Vec<Tab>,                                  // The tabs in the application
    active_tab_index: Option<usize>,                 // Index of the active tab
    next_tab_id: usize,                              // The next tab ID
    pub search_state: SearchState,                   // Search and replace functionality state
    pub color_picker_bar_state: ColorPickerBarState, // Color picker bar state
    pub jump_to_line_input: Entity<InputState>,      // Input for jumping to a line in the editor
    pending_jump: Option<editor_tab::Jump>,          // Pending jump to line action
    pub settings: Settings, // The settings for the application (local copy for fast access)
    settings_changed: bool, // Flag to indicate that the settings have been changed and need to be saved
    local_settings_version: u64, // Track the version of settings this window has loaded
    rendered_tabs: HashSet<usize>, // Track which tabs have been rendered
    tabs_pending_update: HashSet<usize>, // Track tabs that need settings update on next render
    editor_modified_subscriptions: HashMap<usize, (EntityId, Subscription)>, // Per-editor (subscribed content entity id, subscription) for incremental modified-state updates
    markdown_preview_cache: HashMap<usize, SharedString>, // Cached markdown source text keyed by source editor tab id
    markdown_preview_to_refresh: HashSet<usize>, // Source tab ids whose cached preview text is stale and must be refreshed on next read
    markdown_preview_subscriptions: HashMap<usize, (EntityId, Subscription)>, // Per-source (subscribed content entity id, subscription) for markdown preview cache updates
    tab_scroll_handle: ScrollHandle, // Scroll handle for the tab bar to scroll active tab into view
    pending_tab_scroll: Option<usize>, // Deferred scroll-to-tab request (needs one render cycle for layout)
    pub file_watch_state: FileWatchState, // File watching state for external file change detection
    pub pending_notification: Option<(NotificationType, SharedString)>, // Pending notification to display on next render
    save_failed_once: bool, // Flag: save already failed once, allow force-close on next attempt
    pub share_sheet_state: Option<Arc<ui::sheets::share_file::ShareSheetState>>, // When Some, a share sheet is open and devices are being fetched per profile
    cached_window_bounds: Option<state::SerializedWindowBounds>, // Cached window bounds for cross-window saves
    font_select_subscription: Option<Subscription>, // Subscription for font family selection events (set when settings tab is opened)
    editor_context_menu: Option<(Point<Pixels>, Entity<PopupMenu>)>, // Custom right-click context menu for the editor
    editor_context_menu_subscription: Option<Subscription>, // Subscription to clear editor_context_menu on dismiss
    drag_ghost: Option<(usize, ui::tabs::tab_drag::DraggedTab)>, // Ghost tab shown at insertion point during tab drag
    status_bar_cache: StatusBarCache, // Cached status bar label strings (refreshed each render)
    cached_tab_filename_counts: HashMap<String, usize>, // Cached tab filename frequency map (refreshed when tabs change)
    tab_filename_fp: u64, // Fingerprint of the tab list used to detect when cached_tab_filename_counts is stale
    pub pending_tab_transfer: Option<editor_tab::TabTransferData>, // Incoming tab state from another window, processed on next render
    pending_tab_removal: Option<usize>, // Tab ID to remove after it has been sent to another window
    pending_transfer_scroll: Option<gpui_component::input::Position>, // Deferred scroll-to-cursor after tab transfer (needs one render cycle for layout)
    pending_remote_open: Arc<parking_lot::Mutex<Vec<PendingRemoteOpenOutcome>>>, // Queue for SSH background threads to deliver loaded remote files
    next_remote_request_id: u64, // Monotonic identifier for remote open/save operations targeting existing tabs
    latest_remote_open_request_by_tab: HashMap<usize, u64>, // Latest remote-open request id expected per tab id
    latest_remote_save_request_by_tab: HashMap<usize, u64>, // Latest remote-save request id expected per tab id
    last_failed_remote_open_url: Option<String>, // Last attempted remote URL kept for retry prefill after connection/open failures
    pending_remote_restore: HashSet<usize>, // Restored remote tab ids that should lazily reconnect on first activation/save
    inflight_remote_restore: HashSet<usize>, // Restored remote tabs currently running a reconnect task
    pending_initial_active_tab: Option<usize>, // Active tab to re-activate after first render so dialogs can open safely
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
