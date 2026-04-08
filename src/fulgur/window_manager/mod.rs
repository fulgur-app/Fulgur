mod dock_menu;
mod lifecycle;
mod render;

use crate::fulgur::Fulgur;
use gpui::{App, Global, WeakEntity, WindowId};
use std::collections::HashMap;
use std::path::PathBuf;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

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
    /// Per-window fingerprint used to detect dock menu / jump list relevant changes
    window_menu_fingerprints: HashMap<WindowId, u64>,
    /// Global revision incremented whenever dock menu / jump list input state changes
    menu_state_revision: u64,
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
            window_menu_fingerprints: HashMap::new(),
            menu_state_revision: 0,
        }
    }

    /// Bump the global menu-state revision.
    ///
    /// This revision is consumed by per-window render code to avoid rebuilding
    /// dock/jump list data when no window state relevant to those menus changed.
    fn bump_menu_state_revision(&mut self) {
        self.menu_state_revision = self.menu_state_revision.wrapping_add(1);
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
        self.bump_menu_state_revision();
    }

    /// Unregister a window when it closes
    ///
    /// ### Arguments
    /// - `window_id`: The ID of the window to unregister
    pub fn unregister(&mut self, window_id: WindowId) {
        log::debug!("Unregistering window {:?}", window_id);
        self.windows.remove(&window_id);
        self.window_names.remove(&window_id);
        self.window_menu_fingerprints.remove(&window_id);

        // Update last_focused if this was the focused window
        if self.last_focused == Some(window_id) {
            self.last_focused = self.windows.keys().next().copied();
        }
        self.bump_menu_state_revision();
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

    /// Update the dock menu / jump list fingerprint for one window.
    ///
    /// ### Arguments
    /// - `window_id`: The window whose fingerprint is being published
    /// - `fingerprint`: Hash of all menu-relevant local window state
    ///
    /// ### Returns
    /// - `true`: Fingerprint changed and global menu revision was bumped
    /// - `false`: Fingerprint unchanged or window is no longer registered
    pub fn update_window_menu_fingerprint(
        &mut self,
        window_id: WindowId,
        fingerprint: u64,
    ) -> bool {
        if !self.windows.contains_key(&window_id) {
            return false;
        }
        if self.window_menu_fingerprints.get(&window_id) == Some(&fingerprint) {
            return false;
        }
        self.window_menu_fingerprints.insert(window_id, fingerprint);
        self.bump_menu_state_revision();
        true
    }

    /// Get the latest published fingerprint for a specific window.
    ///
    /// ### Arguments
    /// - `window_id`: The target window ID
    ///
    /// ### Returns
    /// - `Some(u64)`: Last known fingerprint for this window
    /// - `None`: Window has not published one yet
    pub fn get_window_menu_fingerprint(&self, window_id: WindowId) -> Option<u64> {
        self.window_menu_fingerprints.get(&window_id).copied()
    }

    /// Get the current global dock menu / jump list state revision.
    ///
    /// ### Returns
    /// - `u64`: Monotonic revision that changes when menu input state changes
    pub fn menu_state_revision(&self) -> u64 {
        self.menu_state_revision
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
