use super::WindowManager;
use crate::fulgur::Fulgur;
#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui::BorrowAppContext;
use gpui::{Context, Entity, WeakEntity, Window, WindowId};

impl Fulgur {
    /// Publish this window's menu-relevant tab snapshot to the `WindowManager`.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    pub(super) fn publish_window_menu_tabs_if_changed(&mut self, cx: &mut Context<Self>) {
        use super::system_menus::WindowMenuTab;

        let tabs: Vec<WindowMenuTab> = self
            .tabs
            .iter()
            .map(|tab| {
                let tab = tab.read(cx);
                WindowMenuTab {
                    path: tab
                        .as_editor()
                        .and_then(|editor| editor.file_path())
                        .cloned(),
                    title: tab.title(),
                }
            })
            .collect();
        if cx
            .global::<WindowManager>()
            .get_window_menu_tabs(self.window_id)
            == Some(tabs.as_slice())
        {
            return;
        }
        cx.update_global::<WindowManager, _>(|manager, _| {
            manager.publish_window_menu_tabs(self.window_id, tabs);
        });
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
        if let Some(tab_index) = self.find_tab_by_path(&path, cx) {
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
                if other.find_tab_by_path(&path, cx).is_some() {
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
                            for handle in cx.windows() {
                                if handle.window_id() == target_wid {
                                    handle
                                        .update(cx, |_, target_window, cx| {
                                            target_entity.update(cx, |fulgur, cx| {
                                                if let Some(tab_index) =
                                                    fulgur.find_tab_by_path(&path, cx)
                                                {
                                                    fulgur.set_active_tab(
                                                        tab_index,
                                                        target_window,
                                                        cx,
                                                    );
                                                }
                                            });
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
        if let Some(tab_index) = self.tabs.iter().position(|t| t.read(cx).title() == title) {
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
                if other.tabs.iter().any(|t| t.read(cx).title() == title) {
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
                            for handle in cx.windows() {
                                if handle.window_id() == target_wid {
                                    handle
                                        .update(cx, |_, target_window, cx| {
                                            target_entity.update(cx, |fulgur, cx| {
                                                if let Some(tab_index) = fulgur
                                                    .tabs
                                                    .iter()
                                                    .position(|t| t.read(cx).title() == title)
                                                {
                                                    fulgur.set_active_tab(
                                                        tab_index,
                                                        target_window,
                                                        cx,
                                                    );
                                                }
                                            });
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
            .is_none_or(|id| id == self.window_id);
        if !should_process {
            return;
        }
        let commands: Vec<String> = {
            let shared = Fulgur::shared_state(cx);
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
