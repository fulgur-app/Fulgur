use crate::fulgur::{Fulgur, state_persistence, ui::tabs::editor_tab::TabTransferData};
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
    /// Assigned display names (A, B, ..., Z, AA, ...) per window, allocated on registration
    window_names: HashMap<WindowId, String>,
    /// Monotonically increasing counter used to assign unique names; never resets or reuses
    next_name_index: usize,
}

/// Convert a zero-based index to an alphabetic window name (A, B, ..., Z, AA, AB, ...).
///
/// This follows the same scheme as spreadsheet column labels:
/// 0 → "A", 25 → "Z", 26 → "AA", 27 → "AB", …
///
/// ### Arguments
/// - `index`: The zero-based index to convert
///
/// ### Returns
/// - `String`: The alphabetic name corresponding to the index
fn index_to_name(mut index: usize) -> String {
    let mut name = String::new();
    loop {
        name.insert(0, (b'A' + (index % 26) as u8) as char);
        if index < 26 {
            break;
        }
        index = index / 26 - 1;
    }
    name
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
            window_names: HashMap::new(),
            next_name_index: 0,
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
        let name = index_to_name(self.next_name_index);
        self.next_name_index += 1;
        self.window_names.insert(window_id, name);
    }

    /// Unregister a window when it closes
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to unregister
    pub fn unregister(&mut self, window_id: WindowId) {
        log::debug!("Unregistering window {:?}", window_id);
        self.windows.remove(&window_id);
        self.window_names.remove(&window_id);

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

    /// Return the display name for a window, but only when multiple windows are open.
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window
    ///
    /// ### Returns
    /// - `Some(&str)`: The name (e.g. "A", "B", "AA") when more than one window is open
    /// - `None`: When only one window is open or the ID is unknown
    pub fn get_window_name(&self, window_id: WindowId) -> Option<&str> {
        if self.windows.len() <= 1 {
            return None;
        }
        self.window_names.get(&window_id).map(|s| s.as_str())
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
                    if self.save_failed_once {
                        log::warn!("Save failed again — allowing force-close");
                    } else {
                        self.save_failed_once = true;
                        self.pending_notification = Some((
                            NotificationType::Error,
                            format!(
                                "Failed to save application state: {}. Close again to force-close.",
                                e
                            )
                            .into(),
                        ));
                        cx.notify();
                        return false;
                    }
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
                if self.save_failed_once {
                    log::warn!("Save failed again — allowing force-close");
                } else {
                    self.save_failed_once = true;
                    self.pending_notification = Some((
                        NotificationType::Error,
                        format!(
                            "Failed to save application state: {}. Close again to force-close.",
                            e
                        )
                        .into(),
                    ));
                    cx.notify();
                    return false;
                }
            }
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.unregister(self.window_id);
            });
            // Notify remaining windows so they update their titles (remove or reassign suffix)
            for weak in cx.global::<WindowManager>().get_all_windows() {
                if let Some(entity) = weak.upgrade() {
                    entity.update(cx, |_, cx| cx.notify());
                }
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
                    let window_id = window.window_handle().window_id();
                    let view = Fulgur::new(window, cx, window_id, usize::MAX); // usize::MAX = new empty window
                    cx.update_global::<WindowManager, _>(|manager, _| {
                        manager.register(window_id, view.downgrade());
                    });
                    // Notify all windows so they update their titles to include the window name
                    for weak in cx.global::<WindowManager>().get_all_windows() {
                        if let Some(entity) = weak.upgrade() {
                            entity.update(cx, |_, cx| cx.notify());
                        }
                    }
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

    /// Open a new Fulgur window and transfer a tab into it on the first render.
    ///
    /// Behaves like `open_new_window` but sets `pending_tab_transfer` on the new
    /// window entity before the first render cycle, so the tab lands in the new
    /// window as if it had been sent via the normal cross-window transfer path.
    ///
    /// ### Arguments
    /// - `data` - The serialized tab state to transfer
    /// - `cx` - The context for the application
    pub fn open_new_window_with_tab(&self, data: TabTransferData, cx: &mut Context<Self>) {
        let async_cx = cx.to_async();
        async_cx
            .spawn(async move |cx| {
                let window_options = WindowOptions {
                    titlebar: Some(gpui_component::TitleBar::title_bar_options()),
                    #[cfg(target_os = "linux")]
                    window_decorations: Some(gpui::WindowDecorations::Client),
                    ..Default::default()
                };
                let window = cx.open_window(window_options, move |window, cx| {
                    window.set_window_title("Fulgur");
                    let window_id = window.window_handle().window_id();
                    let view = Fulgur::new(window, cx, window_id, usize::MAX - 1);
                    cx.update_global::<WindowManager, _>(|manager, _| {
                        manager.register(window_id, view.downgrade());
                    });
                    for weak in cx.global::<WindowManager>().get_all_windows() {
                        if let Some(entity) = weak.upgrade() {
                            entity.update(cx, |_, cx| cx.notify());
                        }
                    }
                    let view_clone = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        view_clone.update(cx, |fulgur, cx| {
                            fulgur.on_window_close_requested(window, cx)
                        })
                    });
                    view.update(cx, |fulgur, cx| {
                        fulgur.pending_tab_transfer = Some(data);
                        cx.notify();
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
    /// Computes a hash of the current state (all tabs across all windows and recent files)
    /// and only rebuilds the dock menu when the hash differs from the last known state.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    #[cfg(target_os = "macos")]
    fn update_dock_menu_if_changed(&mut self, cx: &mut Context<Self>) {
        use crate::fulgur::ui::menus::{DockMenuTab, build_dock_menu};
        let mut hasher = std::collections::hash_map::DefaultHasher::new();

        // Collect raw tab data from all windows: (file_path_or_none, title, window_group_index)
        // We need all file paths upfront to compute cross-window duplicate filenames.
        struct RawTab {
            path: Option<PathBuf>,
            title: SharedString,
        }

        let mut all_windows_raw: Vec<Vec<RawTab>> = Vec::new();
        let current_window_raw: Vec<RawTab> = self
            .tabs
            .iter()
            .map(|tab| {
                let path = tab.as_editor().and_then(|e| e.file_path.clone());
                if let Some(ref p) = path {
                    p.hash(&mut hasher);
                }
                tab.title().hash(&mut hasher);
                RawTab {
                    path,
                    title: tab.title(),
                }
            })
            .collect();
        all_windows_raw.push(current_window_raw);
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
                let window_raw: Vec<RawTab> = other
                    .tabs
                    .iter()
                    .map(|tab| {
                        let path = tab.as_editor().and_then(|e| e.file_path.clone());
                        if let Some(ref p) = path {
                            p.hash(&mut hasher);
                        }
                        tab.title().hash(&mut hasher);
                        RawTab {
                            path,
                            title: tab.title(),
                        }
                    })
                    .collect();
                all_windows_raw.push(window_raw);
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
        let all_file_paths: Vec<PathBuf> = all_windows_raw
            .iter()
            .flat_map(|w| w.iter())
            .filter_map(|t| t.path.clone())
            .collect();
        let display_name_for_path = |path: &PathBuf| -> SharedString {
            let filename = path
                .file_name()
                .and_then(|n| n.to_str())
                .unwrap_or("Untitled");
            let has_duplicate = all_file_paths.iter().any(|other_path| {
                other_path != path
                    && (other_path.file_name().and_then(|n| n.to_str()) == Some(filename))
            });
            if has_duplicate
                && let Some(parent_name) = path
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
            {
                return SharedString::from(format!("{} (../{})", filename, parent_name));
            }
            SharedString::from(filename.to_string())
        };
        let windows: Vec<Vec<DockMenuTab>> = all_windows_raw
            .into_iter()
            .map(|window_raw| {
                window_raw
                    .into_iter()
                    .map(|raw| match raw.path {
                        Some(path) => DockMenuTab::File {
                            name: display_name_for_path(&path),
                            path,
                        },
                        None => DockMenuTab::Titled {
                            name: raw.title.clone(),
                            title: raw.title,
                        },
                    })
                    .collect()
            })
            .collect();
        let dock_items = build_dock_menu(&windows, &recent_files);
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

    /// Handle the DockActivateTabByTitle action: focus the tab with the given title,
    /// switching to the correct window if necessary.
    ///
    /// ### Arguments
    /// - `action`: The DockActivateTabByTitle action containing the tab title
    /// - `window`: The current window
    /// - `cx`: The application context
    pub fn handle_dock_activate_tab_by_title(
        &mut self,
        action: &crate::fulgur::ui::menus::DockActivateTabByTitle,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let title = action.0.clone();
        if let Some(tab_index) = self.tabs.iter().position(|t| t.title() == title) {
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
                if other.tabs.iter().any(|t| t.title() == title) {
                    target = Some((other.window_id, entity.clone()));
                    break;
                }
            }
        }
        if let Some((target_wid, target_entity)) = target {
            cx.spawn_in(window, {
                async move |_this, async_window| {
                    async_window
                        .update(|_window, cx| {
                            target_entity.update(cx, |fulgur, cx| {
                                if let Some(tab_index) =
                                    fulgur.tabs.iter().position(|t| t.title() == title)
                                {
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{Fulgur, WindowManager};
    use crate::fulgur::{settings::Settings, shared_state::SharedAppState};
    use gpui::{AppContext, BorrowAppContext, Entity, SharedString, TestAppContext, WindowId};
    use gpui_component::notification::NotificationType;
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    /// Initialize test globals required by `Fulgur::new`.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context to initialize.
    fn setup_test_globals(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
    }

    /// Open a window that owns a `Fulgur` entity and return both identifiers.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context used to open the window.
    ///
    /// ### Returns
    /// - `(WindowId, Entity<Fulgur>)`: The window ID and the associated `Fulgur` entity.
    fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
        let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        cx.update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                *window_id_slot.borrow_mut() = Some(window_id);
                *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
            })
            .expect("failed to open test window");
        });
        (
            window_id_slot
                .into_inner()
                .expect("failed to capture test window id"),
            fulgur_slot
                .into_inner()
                .expect("failed to capture test Fulgur entity"),
        )
    }

    /// Build an OS-agnostic temporary path for file lookup tests.
    ///
    /// ### Arguments
    /// - `file_name`: The file name to append to the platform temp directory.
    ///
    /// ### Returns
    /// - `PathBuf`: A path rooted under `std::env::temp_dir()`.
    fn temp_test_path(file_name: &str) -> PathBuf {
        std::env::temp_dir().join(file_name)
    }

    /// Register a window entity inside the global `WindowManager`.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `window_id`: The ID of the window to register.
    /// - `fulgur`: The `Fulgur` entity associated with the window.
    fn register_window_in_global_manager(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
    ) {
        cx.update(|cx| {
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.register(window_id, fulgur.downgrade());
            });
        });
    }

    /// Invoke `on_window_close_requested` against a specific window in tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `window_id`: The target window ID to run close handling against.
    /// - `fulgur`: The `Fulgur` entity that owns the close handler.
    ///
    /// ### Returns
    /// - `bool`: The return value from `Fulgur::on_window_close_requested`.
    fn invoke_window_close_requested(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
    ) -> bool {
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == window_id {
                    return handle
                        .update(cx, |_, window, cx| {
                            fulgur.update(cx, |this, cx| this.on_window_close_requested(window, cx))
                        })
                        .expect("failed to run close handler on test window");
                }
            }
            panic!("failed to locate target test window by id");
        })
    }

    /// Invoke `do_open_file` against a specific window in tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `window_id`: The target window ID where the open request should run.
    /// - `fulgur`: The `Fulgur` entity that owns the open handler.
    /// - `path`: The file path to open.
    fn invoke_do_open_file(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
        path: PathBuf,
    ) {
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            fulgur
                                .update(cx, |this, cx| this.do_open_file(window, cx, path.clone()));
                        })
                        .expect("failed to run do_open_file on test window");
                    return;
                }
            }
            panic!("failed to locate target test window by id");
        });
    }

    /// Invoke `process_window_state_updates` against a specific window in tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `window_id`: The target window ID where the render-phase processing should run.
    /// - `fulgur`: The `Fulgur` entity that owns the processing method.
    fn invoke_process_window_state_updates(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
    ) {
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            fulgur.update(cx, |this, cx| {
                                this.process_window_state_updates(window, cx);
                            });
                        })
                        .expect("failed to run process_window_state_updates on test window");
                    return;
                }
            }
            panic!("failed to locate target test window by id");
        });
    }

    /// Invoke `handle_dock_activate_tab` against a specific window in tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `window_id`: The target window ID where the dock action should run.
    /// - `fulgur`: The `Fulgur` entity that owns the dock handler.
    /// - `path`: The file path carried by `DockActivateTab`.
    fn invoke_dock_activate_tab(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
        path: PathBuf,
    ) {
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            fulgur.update(cx, |this, cx| {
                                let action =
                                    crate::fulgur::ui::menus::DockActivateTab(path.clone());
                                this.handle_dock_activate_tab(&action, window, cx);
                            });
                        })
                        .expect("failed to run DockActivateTab on test window");
                    return;
                }
            }
            panic!("failed to locate target test window by id");
        });
    }

    /// Invoke `handle_dock_activate_tab_by_title` against a specific window in tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `window_id`: The target window ID where the dock action should run.
    /// - `fulgur`: The `Fulgur` entity that owns the dock handler.
    /// - `title`: The tab title carried by `DockActivateTabByTitle`.
    fn invoke_dock_activate_tab_by_title(
        cx: &mut TestAppContext,
        window_id: WindowId,
        fulgur: &Entity<Fulgur>,
        title: SharedString,
    ) {
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            fulgur.update(cx, |this, cx| {
                                let action =
                                    crate::fulgur::ui::menus::DockActivateTabByTitle(title.clone());
                                this.handle_dock_activate_tab_by_title(&action, window, cx);
                            });
                        })
                        .expect("failed to run DockActivateTabByTitle on test window");
                    return;
                }
            }
            panic!("failed to locate target test window by id");
        });
    }

    #[gpui::test]
    fn test_register_unregister_and_focus_tracking(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        cx.update(|_| {
            let mut manager = WindowManager::new();
            assert_eq!(manager.window_count(), 0);
            assert_eq!(manager.get_last_focused(), None);
            manager.register(window_id_one, fulgur_one.downgrade());
            assert_eq!(manager.window_count(), 1);
            assert_eq!(manager.get_last_focused(), Some(window_id_one));
            assert!(manager.get_window(window_id_one).is_some());
            manager.register(window_id_two, fulgur_two.downgrade());
            assert_eq!(manager.window_count(), 2);
            assert_eq!(manager.get_last_focused(), Some(window_id_two));
            let window_ids = manager.get_all_window_ids();
            assert_eq!(window_ids.len(), 2);
            assert!(window_ids.contains(&window_id_one));
            assert!(window_ids.contains(&window_id_two));
            assert_eq!(manager.get_all_windows().len(), 2);
            manager.set_focused(window_id_one);
            assert_eq!(manager.get_last_focused(), Some(window_id_one));
            manager.unregister(window_id_one);
            assert_eq!(manager.window_count(), 1);
            assert_eq!(manager.get_last_focused(), Some(window_id_two));
            assert!(manager.get_window(window_id_one).is_none());
            // Focusing an unregistered window must leave focus unchanged.
            manager.set_focused(window_id_one);
            assert_eq!(manager.get_last_focused(), Some(window_id_two));
            manager.unregister(window_id_two);
            assert_eq!(manager.window_count(), 0);
            assert_eq!(manager.get_last_focused(), None);
        });
    }

    #[gpui::test]
    fn test_find_window_with_file_returns_other_window_with_matching_tab(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
        let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
        let target_path = temp_test_path("fulgur_window_manager_cross_window_lookup.md");
        cx.update(|cx| {
            other_fulgur.update(cx, |fulgur, _| {
                let editor = fulgur
                    .tabs
                    .first_mut()
                    .and_then(|tab| tab.as_editor_mut())
                    .expect("expected initial editor tab");
                editor.file_path = Some(target_path.clone());
            });
            let mut manager = WindowManager::new();
            manager.register(current_window_id, current_fulgur.downgrade());
            manager.register(other_window_id, other_fulgur.downgrade());
            let found_window_id =
                manager.find_window_with_file(&target_path, current_window_id, cx);
            assert_eq!(found_window_id, Some(other_window_id));
        });
    }

    #[gpui::test]
    fn test_find_window_with_file_skips_current_window_and_returns_none_on_miss(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
        let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
        let current_only_path = temp_test_path("fulgur_window_manager_current_only.rs");
        let missing_path = temp_test_path("fulgur_window_manager_missing.rs");
        cx.update(|cx| {
            current_fulgur.update(cx, |fulgur, _| {
                let editor = fulgur
                    .tabs
                    .first_mut()
                    .and_then(|tab| tab.as_editor_mut())
                    .expect("expected initial editor tab");
                editor.file_path = Some(current_only_path.clone());
            });
            let mut manager = WindowManager::new();
            manager.register(current_window_id, current_fulgur.downgrade());
            manager.register(other_window_id, other_fulgur.downgrade());
            let found_in_other =
                manager.find_window_with_file(&current_only_path, current_window_id, cx);
            assert_eq!(
                found_in_other, None,
                "current window must be ignored during cross-window lookup"
            );
            let missing = manager.find_window_with_file(&missing_path, current_window_id, cx);
            assert_eq!(
                missing, None,
                "missing files should return no matching window"
            );
        });
    }

    #[gpui::test]
    fn test_on_window_close_requested_last_window_with_confirm_exit_blocks_close(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id, fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, window_id, &fulgur);
        cx.update(|cx| {
            fulgur.update(cx, |this, _| {
                this.settings.app_settings.confirm_exit = true;
            });
        });
        let should_close = invoke_window_close_requested(cx, window_id, &fulgur);
        assert!(
            !should_close,
            "last window should remain open when confirm_exit is enabled"
        );
        cx.update(|cx| {
            let manager = cx.global::<WindowManager>();
            assert_eq!(manager.window_count(), 1);
            assert!(manager.get_window(window_id).is_some());
        });
    }

    #[gpui::test]
    fn test_on_window_close_requested_last_window_without_confirm_exit_closes_and_unregisters(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id, fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, window_id, &fulgur);
        cx.update(|cx| {
            fulgur.update(cx, |this, _| {
                this.settings.app_settings.confirm_exit = false;
            });
        });
        let should_close = invoke_window_close_requested(cx, window_id, &fulgur);
        assert!(
            should_close,
            "last window should close when confirm_exit is disabled"
        );
        cx.update(|cx| {
            let manager = cx.global::<WindowManager>();
            assert_eq!(manager.window_count(), 0);
            assert!(manager.get_window(window_id).is_none());
        });
    }

    #[gpui::test]
    fn test_on_window_close_requested_non_last_window_closes_even_with_confirm_exit_enabled(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id_one, fulgur_one) = open_window_with_fulgur(cx);
        let (window_id_two, fulgur_two) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, window_id_one, &fulgur_one);
        register_window_in_global_manager(cx, window_id_two, &fulgur_two);
        cx.update(|cx| {
            fulgur_two.update(cx, |this, _| {
                this.settings.app_settings.confirm_exit = true;
            });
        });
        let should_close = invoke_window_close_requested(cx, window_id_two, &fulgur_two);
        assert!(
            should_close,
            "non-last windows should close without quit confirmation flow"
        );
        cx.update(|cx| {
            let manager = cx.global::<WindowManager>();
            assert_eq!(manager.window_count(), 1);
            assert!(manager.get_window(window_id_one).is_some());
            assert!(manager.get_window(window_id_two).is_none());
            assert_eq!(manager.get_last_focused(), Some(window_id_one));
        });
    }

    #[gpui::test]
    fn test_process_window_state_updates_drains_local_and_sync_pending_notifications(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (window_id, fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, window_id, &fulgur);

        cx.update(|cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_notification = Some((
                    NotificationType::Warning,
                    "pending from current window".into(),
                ));
                *this.shared_state(cx).sync_state.pending_notification.lock() = Some((
                    NotificationType::Success,
                    "pending from sync background task".into(),
                ));
            });
        });

        invoke_process_window_state_updates(cx, window_id, &fulgur);

        cx.update(|cx| {
            fulgur.update(cx, |this, cx| {
                assert!(
                    this.pending_notification.is_none(),
                    "window-local pending notification must be drained during render processing"
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_notification
                        .lock()
                        .is_none(),
                    "sync pending notification must be drained during render processing"
                );
            });

            let manager = cx.global::<WindowManager>();
            assert_eq!(
                manager.get_last_focused(),
                Some(window_id),
                "process_window_state_updates should keep focus tracking in sync"
            );
        });
    }

    #[gpui::test]
    fn test_do_open_file_does_not_open_duplicate_when_file_exists_in_another_window(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
        let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, current_window_id, &current_fulgur);
        register_window_in_global_manager(cx, other_window_id, &other_fulgur);
        let shared_path = temp_test_path("fulgur_cross_window_existing_file.rs");
        cx.update(|cx| {
            other_fulgur.update(cx, |fulgur, _| {
                let editor = fulgur
                    .tabs
                    .first_mut()
                    .and_then(|tab| tab.as_editor_mut())
                    .expect("expected initial editor tab");
                editor.file_path = Some(shared_path.clone());
            });
        });
        let tab_count_before = cx.update(|cx| current_fulgur.read(cx).tabs.len());
        invoke_do_open_file(cx, current_window_id, &current_fulgur, shared_path.clone());
        cx.run_until_parked();
        let tab_count_after = cx.update(|cx| current_fulgur.read(cx).tabs.len());
        assert_eq!(
            tab_count_after, tab_count_before,
            "opening a file already open in another window should not create a duplicate tab"
        );
    }

    #[gpui::test]
    fn test_dock_activate_tab_transfers_active_tab_to_other_window(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
        let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, current_window_id, &current_fulgur);
        register_window_in_global_manager(cx, other_window_id, &other_fulgur);
        let target_path = temp_test_path("fulgur_dock_focus_transfer.rs");
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == other_window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            other_fulgur.update(cx, |this, cx| {
                                this.new_tab(window, cx);
                                this.active_tab_index = Some(0);
                                if let Some(editor) =
                                    this.tabs.get_mut(1).and_then(|tab| tab.as_editor_mut())
                                {
                                    editor.file_path = Some(target_path.clone());
                                    editor.title = "dock-target.rs".into();
                                }
                            });
                        })
                        .expect("failed to prepare target window tab state");
                    break;
                }
            }
        });

        invoke_dock_activate_tab(cx, current_window_id, &current_fulgur, target_path.clone());
        cx.run_until_parked();

        cx.update(|cx| {
            let other = other_fulgur.read(cx);
            assert_eq!(
                other.active_tab_index,
                Some(1),
                "dock activation by path should activate the matching tab in the other window"
            );
        });
    }

    #[gpui::test]
    fn test_dock_activate_tab_by_title_transfers_active_tab_to_other_window(
        cx: &mut TestAppContext,
    ) {
        setup_test_globals(cx);
        let (current_window_id, current_fulgur) = open_window_with_fulgur(cx);
        let (other_window_id, other_fulgur) = open_window_with_fulgur(cx);
        register_window_in_global_manager(cx, current_window_id, &current_fulgur);
        register_window_in_global_manager(cx, other_window_id, &other_fulgur);
        let target_title: SharedString = "cross-window-title-target".into();
        cx.update(|cx| {
            for handle in cx.windows() {
                if handle.window_id() == other_window_id {
                    handle
                        .update(cx, |_, window, cx| {
                            other_fulgur.update(cx, |this, cx| {
                                this.new_tab(window, cx);
                                this.active_tab_index = Some(0);
                                if let Some(editor) =
                                    this.tabs.get_mut(1).and_then(|tab| tab.as_editor_mut())
                                {
                                    editor.title = target_title.clone();
                                }
                            });
                        })
                        .expect("failed to prepare target window title state");
                    break;
                }
            }
        });
        invoke_dock_activate_tab_by_title(
            cx,
            current_window_id,
            &current_fulgur,
            target_title.clone(),
        );
        cx.run_until_parked();

        cx.update(|cx| {
            let other = other_fulgur.read(cx);
            assert_eq!(
                other.active_tab_index,
                Some(1),
                "dock activation by title should activate the matching tab in the other window"
            );
        });
    }
}
