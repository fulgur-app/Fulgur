use super::WindowManager;
use crate::fulgur::Fulgur;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui::BorrowAppContext;
use gpui::{Context, Entity, WeakEntity, Window, WindowId};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::hash::{Hash, Hasher};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use std::path::PathBuf;

impl Fulgur {
    /// Compute a lightweight fingerprint for this window's dock/jump-list inputs.
    ///
    /// Includes all local tabs (id/title/path) and recent files to detect when
    /// system menu state may need rebuilding.
    ///
    /// ### Returns
    /// - `u64`: Fingerprint of this window's menu-relevant state
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    fn compute_local_menu_fingerprint(&mut self) -> u64 {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        for tab in &self.tabs {
            tab.id().hash(&mut hasher);
            tab.title().hash(&mut hasher);
            if let Some(path) = tab.as_editor().and_then(|editor| editor.file_path()) {
                path.hash(&mut hasher);
            }
        }
        for file in self.settings.get_recent_files() {
            file.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Publish this window's local menu fingerprint to the `WindowManager`.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn publish_window_menu_fingerprint_if_changed(&mut self, cx: &mut Context<Self>) {
        let fingerprint = self.compute_local_menu_fingerprint();
        let already_published = cx
            .global::<WindowManager>()
            .get_window_menu_fingerprint(self.window_id)
            == Some(fingerprint);
        if fingerprint == self.local_window_menu_fingerprint && already_published {
            return;
        }
        self.local_window_menu_fingerprint = fingerprint;
        cx.update_global::<WindowManager, _>(|manager, _| {
            manager.update_window_menu_fingerprint(self.window_id, fingerprint);
        });
    }

    /// Update the macOS dock menu if the open tabs or recent files have changed.
    ///
    /// Computes a hash of the current state (all tabs across all windows and recent files)
    /// and only rebuilds the dock menu when the hash differs from the last known state.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    #[cfg(target_os = "macos")]
    pub(super) fn update_dock_menu_if_changed(&mut self, cx: &mut Context<Self>) {
        use gpui::{SharedString, WeakEntity};

        use crate::fulgur::ui::menus::{DockMenuTab, build_dock_menu};
        let menu_state_revision = cx.global::<WindowManager>().menu_state_revision();
        if menu_state_revision == self.last_dock_menu_revision {
            return;
        }
        self.last_dock_menu_revision = menu_state_revision;
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
                let path = tab.as_editor().and_then(|e| e.file_path().cloned());
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
                        let path = tab.as_editor().and_then(|e| e.file_path().cloned());
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
                return SharedString::from(format!("{filename} (../{parent_name})"));
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

    /// Update the Windows taskbar Jump List if the open tabs or recent files have changed.
    ///
    /// Mirrors `update_dock_menu_if_changed`: computes a hash of all tabs across all
    /// windows plus recent files and only rebuilds when the hash differs.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    #[cfg(target_os = "windows")]
    pub(super) fn update_jump_list_if_changed(&mut self, cx: &mut Context<Self>) {
        use crate::fulgur::ui::menus::DockMenuTab;
        use crate::fulgur::utils::jump_list::update_windows_jump_list;
        let menu_state_revision = cx.global::<WindowManager>().menu_state_revision();
        if menu_state_revision == self.last_jump_list_revision {
            return;
        }
        self.last_jump_list_revision = menu_state_revision;
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        use gpui::SharedString;

        struct RawTab {
            path: Option<PathBuf>,
            title: SharedString,
        }

        let mut all_windows_raw: Vec<Vec<RawTab>> = Vec::new();
        let current_window_raw: Vec<RawTab> = self
            .tabs
            .iter()
            .map(|tab| {
                let path = tab.as_editor().and_then(|e| e.file_path().cloned());
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
                        let path = tab.as_editor().and_then(|e| e.file_path().cloned());
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
        if hash == self.last_jump_list_hash {
            return;
        }
        self.last_jump_list_hash = hash;
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
                return SharedString::from(format!("{filename} (../{parent_name})"));
            }
            SharedString::from(filename.to_string())
        };
        let windows_data: Vec<Vec<DockMenuTab>> = all_windows_raw
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
        update_windows_jump_list(&windows_data, &recent_files);
    }

    /// Handle the `DockActivateTab` action: focus the tab with the given file path,
    /// switching to the correct window if necessary.
    ///
    /// ### Arguments
    /// - `action`: The `DockActivateTab` action containing the file path
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

    /// Handle the `DockActivateTabByTitle` action: focus the tab with the given title,
    /// switching to the correct window if necessary.
    ///
    /// ### Arguments
    /// - `action`: The `DockActivateTabByTitle` action containing the tab title
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

    /// Process pending IPC commands from the Windows jump list listener
    ///
    /// Drains the `pending_ipc_commands` queue and executes each command
    /// in-process. Only the last-focused window processes the queue, mirroring
    /// the behaviour of `process_pending_files_from_macos`.
    ///
    /// ### Arguments
    /// - `window`: The window being rendered
    /// - `cx`: The application context
    #[cfg(target_os = "windows")]
    pub fn process_pending_ipc_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let should_process = cx
            .global::<WindowManager>()
            .get_last_focused()
            .map(|id| id == self.window_id)
            .unwrap_or(true);
        if !should_process {
            return;
        }
        let commands: Vec<String> = {
            let shared = self.shared_state(cx);
            if let Some(mut q) = shared.pending_ipc_commands.try_lock() {
                q.drain(..).collect()
            } else {
                Vec::new()
            }
        };
        for cmd in commands {
            match cmd.as_str() {
                "new-tab" => {
                    log::info!("IPC: opening new tab");
                    self.new_tab(window, cx);
                }
                "new-window" => {
                    log::info!("IPC: opening new window");
                    self.open_new_window(cx);
                }
                other => {
                    log::warn!("IPC: unknown command '{other}'");
                }
            }
        }
    }
}
