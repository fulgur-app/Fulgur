use super::WindowManager;
use crate::fulgur::{Fulgur, tab::Tab};
#[cfg(any(target_os = "macos", target_os = "windows"))]
use gpui_kit::BorrowAppContext;
use gpui_kit::{App, Context, Entity, Window, WindowId};

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
        self.activate_tab_matching(move |tab| tab.has_file_path(&path), window, cx);
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
        self.activate_tab_matching(move |tab| tab.title() == title, window, cx);
    }

    /// Activate the first tab satisfying a predicate, searching this window first
    ///
    /// ### Arguments
    /// - `matches`: The predicate identifying the tab to activate
    /// - `window`: The current window
    /// - `cx`: The application context
    fn activate_tab_matching(
        &mut self,
        matches: impl Fn(&Tab) -> bool + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Some(tab_index) = self.find_tab_matching(&matches, cx) {
            self.set_active_tab(tab_index, window, cx);
            self.focus_active_tab(window, cx);
            cx.notify();
            return;
        }
        let Some((target_wid, target_entity)) = self.find_other_window_with_tab(&matches, cx)
        else {
            return;
        };
        cx.spawn_in(window, async move |_this, async_window| {
            async_window
                .update(|_window, cx| {
                    let Some(handle) = cx
                        .windows()
                        .into_iter()
                        .find(|handle| handle.window_id() == target_wid)
                    else {
                        return;
                    };
                    handle
                        .update(cx, |_, target_window, cx| {
                            target_entity.update(cx, |fulgur, cx| {
                                if let Some(tab_index) = fulgur.find_tab_matching(&matches, cx) {
                                    fulgur.set_active_tab(tab_index, target_window, cx);
                                }
                            });
                            target_window.activate_window();
                        })
                        .ok();
                })
                .ok();
        })
        .detach();
    }

    /// Find another open window holding a tab that satisfies a predicate
    ///
    /// ### Arguments
    /// - `matches`: The predicate identifying the wanted tab
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some((WindowId, Entity<Fulgur>))`: The first other window with a matching tab
    /// - `None`: If no other live window holds a matching tab
    fn find_other_window_with_tab(
        &self,
        matches: &impl Fn(&Tab) -> bool,
        cx: &App,
    ) -> Option<(WindowId, Entity<Fulgur>)> {
        let manager = cx.global::<WindowManager>();
        manager
            .get_all_window_ids()
            .into_iter()
            .filter(|id| *id != self.window_id)
            .filter_map(|id| manager.get_window(id)?.upgrade())
            .find(|entity| entity.read(cx).find_tab_matching(matches, cx).is_some())
            .map(|entity| (entity.read(cx).window_id, entity))
    }

    /// Process pending IPC commands forwarded by another Fulgur process
    ///
    /// Commands come from the Windows jump list ("new-tab", "new-window") and from
    /// launches without a file ("focus", already handled by activating the
    /// window). Drains the `pending_ipc_commands` queue and executes each command
    /// in this window, which `deliver_external_open_requests` has chosen.
    ///
    /// ### Arguments
    /// - `window`: The window receiving the commands
    /// - `cx`: The application context
    pub fn process_pending_ipc_commands(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let commands: Vec<String> = Fulgur::shared_state(cx)
            .pending_ipc_commands
            .lock()
            .drain(..)
            .collect();
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
                "focus" => {
                    log::info!("IPC: focusing window");
                }
                other => {
                    log::warn!("IPC: unknown command '{other}'");
                }
            }
        }
    }
}
