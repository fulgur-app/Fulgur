use crate::fulgur::{Fulgur, state_persistence};
use gpui::*;
use gpui_component::WindowExt;
use gpui_component::notification::NotificationType;
use std::collections::HashMap;
#[cfg(target_os = "macos")]
use std::hash::{Hash, Hasher};
use std::path::PathBuf;

/// Window manager tracks all open Fulgur windows and provides cross-window operations
pub struct WindowManager {
    /// All open windows mapped by their window_id
    windows: HashMap<WindowId, WeakEntity<Fulgur>>,
    /// The last focused window for file opening
    last_focused: Option<WindowId>,
}

impl Global for WindowManager {}

impl WindowManager {
    /// Create a new window manager
    ///
    /// ### Returns
    /// - `WindowManager`: A new window manager instance
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            last_focused: None,
        }
    }

    /// Register a new window
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to register
    /// - `entity`: The entity of the window to register
    pub fn register(&mut self, window_id: WindowId, entity: WeakEntity<Fulgur>) {
        log::debug!("Registering window {:?}", window_id);
        self.windows.insert(window_id, entity);
        self.last_focused = Some(window_id);
    }

    /// Unregister a window when it closes
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to unregister
    pub fn unregister(&mut self, window_id: WindowId) {
        log::debug!("Unregistering window {:?}", window_id);
        self.windows.remove(&window_id);

        // Update last_focused if this was the focused window
        if self.last_focused == Some(window_id) {
            self.last_focused = self.windows.keys().next().copied();
        }
    }

    /// Update last focused window
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to focus
    pub fn set_focused(&mut self, window_id: WindowId) {
        if self.windows.contains_key(&window_id) {
            self.last_focused = Some(window_id);
        }
    }

    /// Get the last focused window
    ///
    /// ### Returns
    /// - `Some(WindowId)`: The ID of the last focused window,
    /// - `None`: If no window is focused
    pub fn get_last_focused(&self) -> Option<WindowId> {
        self.last_focused
    }

    /// Get count of open windows
    ///
    /// ### Returns
    /// - `usize`: The number of open windows
    pub fn window_count(&self) -> usize {
        self.windows.len()
    }

    /// Get all window entities
    ///
    /// ### Returns
    /// - `Vec<WeakEntity<Fulgur>>`: A vector of weak references to all open windows
    pub fn get_all_windows(&self) -> Vec<WeakEntity<Fulgur>> {
        self.windows.values().cloned().collect()
    }

    /// Get all window IDs
    ///
    /// ### Returns
    /// - `Vec<WindowId>`: A vector of all window IDs
    pub fn get_all_window_ids(&self) -> Vec<WindowId> {
        self.windows.keys().copied().collect()
    }

    /// Get a specific window entity by ID
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to get
    ///
    /// ### Returns
    /// - `Option<WeakEntity<Fulgur>>`: The window entity if it exists
    pub fn get_window(&self, window_id: WindowId) -> Option<WeakEntity<Fulgur>> {
        self.windows.get(&window_id).cloned()
    }

    /// Find window that has file open
    ///
    /// ### Arguments
    /// - `path`: The path of the file to search for
    /// - `current_window_id`: The current window ID to skip (can't read while already borrowed)
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Option<WindowId>`: The ID of the window that has the file open, if any
    pub fn find_window_with_file(
        &self,
        path: &PathBuf,
        current_window_id: WindowId,
        cx: &App,
    ) -> Option<WindowId> {
        for (window_id, weak_entity) in &self.windows {
            // Skip the current window since it's already borrowed in the render context
            if *window_id == current_window_id {
                continue;
            }

            if let Some(entity) = weak_entity.upgrade() {
                let read = entity.read(cx);
                if read.find_tab_by_path(path).is_some() {
                    return Some(*window_id);
                }
            }
        }
        None
    }
}

impl Default for WindowManager {
    /// Create a default window manager
    ///
    /// ### Returns
    /// - `WindowManager`: A new window manager instance
    fn default() -> Self {
        Self::new()
    }
}

impl Fulgur {
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
        let window_count = cx.global::<WindowManager>().window_count();
        if window_count == 1 {
            if self.settings.app_settings.confirm_exit {
                self.quit(window, cx);
                false
            } else {
                if let Err(e) = self.save_state(cx, window) {
                    log::error!("Failed to save app state on window close: {}", e);
                    self.pending_notification = Some((
                        NotificationType::Error,
                        format!("Failed to save application state: {}. Close anyway?", e).into(),
                    ));
                    cx.notify();
                    return false; // Prevent close, let user try again or force close
                }
                cx.update_global::<WindowManager, _>(|manager, _| {
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
            if let Err(e) = self.save_state(cx, window) {
                log::error!("Failed to save app state on window close: {}", e);
                self.pending_notification = Some((
                    NotificationType::Error,
                    format!("Failed to save application state: {}. Close anyway?", e).into(),
                ));
                cx.notify();
                return false; // Prevent close, let user try again or force close
            }
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.unregister(self.window_id);
            });
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
                    cx.update_global::<WindowManager, _>(|manager, _| {
                        manager.register(window_id, view.downgrade());
                    });
                    let view_clone = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        view_clone.update(cx, |fulgur, cx| {
                            fulgur.on_window_close_requested(window, cx)
                        })
                    });
                    view.update(cx, |fulgur, cx| fulgur.focus_active_tab(window, cx));
                    cx.new(|cx| gpui_component::Root::new(view, window, cx))
                })?;
                window.update(cx, |_, window, _| {
                    window.activate_window();
                })?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
    }

    /// Process window state updates during the render cycle:
    /// 1. Cache the current window bounds and display ID for state persistence
    /// 2. Update the global WindowManager to track this window as focused
    /// 3. Display any pending notifications that were queued during event processing
    ///
    /// ### Arguments
    /// - `window`: The window being rendered
    /// - `cx`: The application context
    pub fn process_window_state_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let display_id = window.display(cx).map(|d| d.id().into());
        self.cached_window_bounds =
            Some(state_persistence::SerializedWindowBounds::from_gpui_bounds(
                window.window_bounds(),
                display_id,
            ));
        cx.update_global::<WindowManager, _>(|manager, _| {
            manager.set_focused(self.window_id);
        });
        if let Some((notification_type, message)) = self.pending_notification.take() {
            window.push_notification((notification_type, message), cx);
        }
        // Check for notifications from background sync operations
        let sync_notification = self
            .shared_state(cx)
            .sync_state
            .pending_notification
            .lock()
            .take();
        if let Some((notification_type, message)) = sync_notification {
            window.push_notification((notification_type, message), cx);
        }
        #[cfg(target_os = "macos")]
        self.update_dock_menu_if_changed(cx);
    }

    /// Update the macOS dock menu if the open tabs or recent files have changed.
    ///
    /// Computes a hash of the current state (open tab paths across all windows
    /// and recent files) and only rebuilds the dock menu when the hash differs
    /// from the last known state.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    #[cfg(target_os = "macos")]
    fn update_dock_menu_if_changed(&mut self, cx: &mut Context<Self>) {
        use crate::fulgur::tab::Tab;
        use crate::fulgur::ui::menus::build_dock_menu;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        let mut open_tabs: Vec<(SharedString, PathBuf)> = Vec::new();
        for tab in &self.tabs {
            if let Tab::Editor(editor_tab) = tab
                && let Some(ref path) = editor_tab.file_path
            {
                path.hash(&mut hasher);
                let name = path
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or("Untitled");
                open_tabs.push((SharedString::from(name.to_string()), path.clone()));
            }
        }
        let manager = cx.global::<WindowManager>();
        let other_windows: Vec<WeakEntity<Fulgur>> = manager
            .get_all_window_ids()
            .into_iter()
            .filter(|id| *id != self.window_id)
            .filter_map(|id| manager.get_window(id))
            .collect();
        for weak in other_windows {
            if let Some(entity) = weak.upgrade() {
                let other = entity.read(cx);
                for tab in &other.tabs {
                    if let Tab::Editor(editor_tab) = tab
                        && let Some(ref path) = editor_tab.file_path
                    {
                        path.hash(&mut hasher);
                        let name = path
                            .file_name()
                            .and_then(|n| n.to_str())
                            .unwrap_or("Untitled");
                        open_tabs.push((SharedString::from(name.to_string()), path.clone()));
                    }
                }
            }
        }
        let recent_files = self.settings.get_recent_files();
        for file in &recent_files {
            file.hash(&mut hasher);
        }
        let hash = hasher.finish();
        if hash == self.last_dock_menu_hash {
            return;
        }
        self.last_dock_menu_hash = hash;
        let dock_items = build_dock_menu(&open_tabs, &recent_files);
        cx.set_dock_menu(dock_items);
    }

    /// Handle the DockActivateTab action: focus the tab with the given file path,
    /// switching to the correct window if necessary.
    ///
    /// ### Arguments
    /// - `action`: The DockActivateTab action containing the file path
    /// - `window`: The current window
    /// - `cx`: The application context
    pub fn handle_dock_activate_tab(
        &mut self,
        action: &crate::fulgur::ui::menus::DockActivateTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let path = action.0.clone();
        if let Some(tab_index) = self.find_tab_by_path(&path) {
            self.set_active_tab(tab_index, window, cx);
            self.focus_active_tab(window, cx);
            cx.notify();
            return;
        }
        let manager = cx.global::<WindowManager>();
        let other_windows: Vec<WeakEntity<Fulgur>> = manager
            .get_all_window_ids()
            .into_iter()
            .filter(|id| *id != self.window_id)
            .filter_map(|id| manager.get_window(id))
            .collect();
        let mut target: Option<(WindowId, Entity<Fulgur>)> = None;
        for weak in other_windows {
            if let Some(entity) = weak.upgrade() {
                let other = entity.read(cx);
                if other.find_tab_by_path(&path).is_some() {
                    target = Some((other.window_id, entity.clone()));
                    break;
                }
            }
        }
        if let Some((target_wid, target_entity)) = target {
            cx.spawn_in(window, {
                let path = path.clone();
                async move |_this, async_window| {
                    async_window
                        .update(|_window, cx| {
                            target_entity.update(cx, |fulgur, cx| {
                                if let Some(tab_index) = fulgur.find_tab_by_path(&path) {
                                    fulgur.tab_scroll_handle.scroll_to_item(tab_index);
                                    fulgur.active_tab_index = Some(tab_index);
                                    cx.notify();
                                }
                            });
                            for handle in cx.windows() {
                                if handle.window_id() == target_wid {
                                    handle
                                        .update(cx, |_, target_window, _| {
                                            target_window.activate_window();
                                        })
                                        .ok();
                                    break;
                                }
                            }
                        })
                        .ok();
                }
            })
            .detach();
        }
    }
}
