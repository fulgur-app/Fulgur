mod dock_menu;
mod lifecycle;
mod render;
pub mod system_menus;

use crate::fulgur::Fulgur;
use gpui::{App, Global, WeakEntity, WindowId};
use std::collections::HashMap;
use std::path::PathBuf;
use system_menus::WindowMenuTab;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

/// Window manager tracks all open Fulgur windows and provides cross-window operations
pub struct WindowManager {
    /// All open windows mapped by their `window_id`
    windows: HashMap<WindowId, WeakEntity<Fulgur>>,
    /// The last focused window for file opening
    last_focused: Option<WindowId>,
    /// Assigned display names (A, B, ..., Z, AA, ...) per window, allocated on registration
    window_names: HashMap<WindowId, String>,
    /// Monotonically increasing counter used to assign unique names; never resets or reuses
    next_name_index: usize,
    /// Menu-relevant tab snapshots published per window, consumed by the system menu observers
    window_menu_tabs: HashMap<WindowId, Vec<WindowMenuTab>>,
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
        name.insert(
            0,
            (b'A' + u8::try_from(index % 26).expect("index % 26 is always < 26")) as char,
        );
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
    #[must_use]
    pub fn new() -> Self {
        Self {
            windows: HashMap::new(),
            last_focused: None,
            window_names: HashMap::new(),
            next_name_index: 0,
            window_menu_tabs: HashMap::new(),
        }
    }

    /// Register a new window
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to register
    /// - `entity`: The entity of the window to register
    pub fn register(&mut self, window_id: WindowId, entity: WeakEntity<Fulgur>) {
        log::debug!("Registering window {window_id:?}");
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
        log::debug!("Unregistering window {window_id:?}");
        self.windows.remove(&window_id);
        self.window_names.remove(&window_id);
        self.window_menu_tabs.remove(&window_id);

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
    #[must_use]
    pub fn get_last_focused(&self) -> Option<WindowId> {
        self.last_focused
    }

    /// Get count of open windows
    ///
    /// ### Returns
    /// - `usize`: The number of open windows
    #[must_use]
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
        self.window_names
            .get(&window_id)
            .map(std::string::String::as_str)
    }

    /// Get all window entities
    ///
    /// ### Returns
    /// - `Vec<WeakEntity<Fulgur>>`: A vector of weak references to all open windows
    #[must_use]
    pub fn get_all_windows(&self) -> Vec<WeakEntity<Fulgur>> {
        self.windows.values().cloned().collect()
    }

    /// Get all window IDs
    ///
    /// ### Returns
    /// - `Vec<WindowId>`: A vector of all window IDs
    #[must_use]
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
    #[must_use]
    pub fn get_window(&self, window_id: WindowId) -> Option<WeakEntity<Fulgur>> {
        self.windows.get(&window_id).cloned()
    }

    /// Publish the menu-relevant tab snapshot for one window.
    ///
    /// ### Arguments
    /// - `window_id`: The window whose snapshot is being published
    /// - `tabs`: Menu-relevant snapshot of the window's tabs, in tab order
    ///
    /// ### Returns
    /// - `true`: Snapshot changed and was stored
    /// - `false`: Snapshot unchanged or window is no longer registered
    pub fn publish_window_menu_tabs(
        &mut self,
        window_id: WindowId,
        tabs: Vec<WindowMenuTab>,
    ) -> bool {
        if !self.windows.contains_key(&window_id) {
            return false;
        }
        if self.window_menu_tabs.get(&window_id) == Some(&tabs) {
            return false;
        }
        self.window_menu_tabs.insert(window_id, tabs);
        true
    }

    /// Get the latest published menu tab snapshot for a specific window.
    ///
    /// ### Arguments
    /// - `window_id`: The target window ID
    ///
    /// ### Returns
    /// - `Some(&[WindowMenuTab])`: Last published snapshot for this window
    /// - `None`: Window has not published one yet
    #[must_use]
    pub fn get_window_menu_tabs(&self, window_id: WindowId) -> Option<&[WindowMenuTab]> {
        self.window_menu_tabs.get(&window_id).map(Vec::as_slice)
    }

    /// Get the published menu tab snapshots of all windows, in registration order.
    ///
    /// ### Returns
    /// - `Vec<Vec<WindowMenuTab>>`: One snapshot per window that has published one
    #[must_use]
    pub fn ordered_window_menu_tabs(&self) -> Vec<Vec<WindowMenuTab>> {
        let mut named_ids: Vec<(&String, WindowId)> = self
            .window_names
            .iter()
            .map(|(window_id, name)| (name, *window_id))
            .collect();
        named_ids.sort_by(|(a, _), (b, _)| a.len().cmp(&b.len()).then_with(|| a.cmp(b)));
        named_ids
            .into_iter()
            .filter_map(|(_, window_id)| self.window_menu_tabs.get(&window_id).cloned())
            .collect()
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
                if read.find_tab_by_path(path, cx).is_some() {
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
