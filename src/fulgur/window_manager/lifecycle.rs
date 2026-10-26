use super::WindowManager;
use crate::fulgur::WindowInit;
use crate::fulgur::ui::dialogs::application_close::CloseScope;
use crate::fulgur::ui::tabs::editor_tab::TabTransferData;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{Fulgur, PendingSaveCloseAction};
use gpui_kit::{App, AppContext, BorrowAppContext, Context, Window, WindowOptions};

impl Fulgur {
    /// Run the guarded window-close lifecycle for an in-app close action.
    ///
    /// ### Arguments
    /// - `window`: The window requested to close
    /// - `cx`: The application context
    pub(crate) fn request_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.on_window_close_requested(window, cx) {
            window.remove_window();
        }
    }

    /// Handle window close request
    ///
    /// ### Description
    /// - If this is the last window: treat as quit, through the application
    ///   close plan when an exit confirmation or a large-file warning is due,
    ///   otherwise save state and let the window close, which quits the app
    /// - If multiple windows exist: confirm unsaved and large modified tabs,
    ///   then save state without this window and close it
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
        if self.defer_close_for_pending_saves(PendingSaveCloseAction::Window) {
            return false;
        }
        let window_count = cx.global::<WindowManager>().window_count();
        if window_count == 1 {
            if self.settings.app_settings.confirm_exit
                || !self.large_modified_local_tabs(cx).is_empty()
            {
                self.quit(window, cx);
                return false;
            }
            return self.save_and_unregister_window(window, cx);
        }
        // A closed non-last window cannot be reopened, so its persisted buffers
        // would be lost; confirm each one like closing the tab would.
        if self.settings.app_settings.persist_unsaved_buffers {
            let unsaved = self.tabs_needing_window_close_prompt(cx);
            if !unsaved.is_empty() {
                self.drive_window_close_unsaved_prompts(unsaved, window, cx);
                return false;
            }
        }
        // Large, modified local files cannot be persisted, so warn about each before closing.
        if !self.large_modified_local_tabs(cx).is_empty() {
            self.start_close_plan(CloseScope::Window, cx);
            return false;
        }
        log::debug!(
            "Closing window {:?} ({} windows remaining)",
            self.window_id,
            window_count - 1
        );
        self.save_and_unregister_window(window, cx)
    }

    /// Finish closing this window programmatically after close warnings.
    ///
    /// ### Arguments
    /// - `window`: The window being closed
    /// - `cx`: The application context
    pub(crate) fn finish_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.save_and_unregister_window(window, cx) {
            window.remove_window();
        }
    }

    /// Persist the session for a closing window, then drop the window from the manager.
    ///
    /// ### Arguments
    /// - `window`: The window being closed
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: The window may close
    /// - `false`: The first session save failure blocks the close
    fn save_and_unregister_window(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if !self.can_close_after_state_save(CloseScope::Window, window, cx) {
            return false;
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

    /// Whether a tab's unsaved changes need confirming before this window closes.
    ///
    /// ### Arguments
    /// - `tab_id`: The tab to check
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: `true` when the tab is a modified editor tab without a large-file warning
    fn tab_needs_window_close_prompt(&self, tab_id: TabId, cx: &App) -> bool {
        self.tab_entity_of(tab_id, cx)
            .and_then(|tab| {
                let tab = tab.read(cx);
                let editor_tab = tab.as_editor()?;
                Some(editor_tab.modified && !editor_tab.needs_large_file_close_warning(cx))
            })
            .unwrap_or(false)
    }

    /// Collect the tabs whose unsaved changes need confirming before this window closes.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<TabId>`: The ids of the tabs to confirm, in tab order
    fn tabs_needing_window_close_prompt(&self, cx: &App) -> Vec<TabId> {
        self.tabs
            .iter()
            .map(|tab| tab.read(cx).id())
            .filter(|tab_id| self.tab_needs_window_close_prompt(*tab_id, cx))
            .collect()
    }

    /// Confirm each unsaved tab in turn, then continue closing this window.
    ///
    /// ### Arguments
    /// - `remaining`: The tabs still to confirm, in order
    /// - `window`: The window being closed
    /// - `cx`: The application context
    fn drive_window_close_unsaved_prompts(
        &mut self,
        remaining: Vec<TabId>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let mut queue = remaining
            .into_iter()
            .skip_while(|tab_id| !self.tab_needs_window_close_prompt(*tab_id, cx));
        let Some(tab_id) = queue.next() else {
            self.start_close_plan(CloseScope::Window, cx);
            return;
        };
        let rest: Vec<TabId> = queue.collect();
        if let Some(index) = self.tab_index_of(tab_id, cx) {
            self.set_active_tab(index, window, cx);
        }
        self.show_unsaved_changes_dialog(window, cx, move |this, window, cx| {
            this.drive_window_close_unsaved_prompts(rest.clone(), window, cx);
        });
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
                    #[cfg(target_os = "linux")]
                    window_decorations: Some(gpui_kit::WindowDecorations::Client),
                    ..gpui_kit::component::TitleBar::window_options()
                };
                let window = cx.open_window(window_options, |window, cx| {
                    window.set_window_title("Fulgur");
                    let window_id = window.window_handle().window_id();
                    let view = Fulgur::new(window, cx, window_id, WindowInit::Empty);
                    cx.update_global::<WindowManager, _>(|manager, _| {
                        manager.register(window_id, view.downgrade());
                    });
                    // Notify all windows so they update their titles to include the window name
                    for weak in cx.global::<WindowManager>().get_all_windows() {
                        if let Some(entity) = weak.upgrade() {
                            entity.update(cx, |_, cx| cx.notify());
                        }
                    }
                    view.update(cx, |fulgur, cx| fulgur.focus_active_tab(window, cx));
                    let root =
                        cx.new(|cx| gpui_kit::component::Root::new(view.clone(), window, cx));
                    let view_clone = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        view_clone.update(cx, |fulgur, cx| {
                            fulgur.on_window_close_requested(window, cx)
                        })
                    });
                    root
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
                    #[cfg(target_os = "linux")]
                    window_decorations: Some(gpui_kit::WindowDecorations::Client),
                    ..gpui_kit::component::TitleBar::window_options()
                };
                let window = cx.open_window(window_options, move |window, cx| {
                    window.set_window_title("Fulgur");
                    let window_id = window.window_handle().window_id();
                    let view = Fulgur::new(window, cx, window_id, WindowInit::AwaitTabTransfer);
                    cx.update_global::<WindowManager, _>(|manager, _| {
                        manager.register(window_id, view.downgrade());
                    });
                    for weak in cx.global::<WindowManager>().get_all_windows() {
                        if let Some(entity) = weak.upgrade() {
                            entity.update(cx, |_, cx| cx.notify());
                        }
                    }
                    view.update(cx, |fulgur, cx| {
                        fulgur.pending_tab_transfer = Some(data);
                        cx.notify();
                    });
                    view.update(cx, |fulgur, cx| fulgur.focus_active_tab(window, cx));
                    let root =
                        cx.new(|cx| gpui_kit::component::Root::new(view.clone(), window, cx));
                    let view_clone = view.clone();
                    window.on_window_should_close(cx, move |window, cx| {
                        view_clone.update(cx, |fulgur, cx| {
                            fulgur.on_window_close_requested(window, cx)
                        })
                    });
                    root
                })?;
                window.update(cx, |_, window, _| {
                    window.activate_window();
                })?;
                Ok::<_, anyhow::Error>(())
            })
            .detach();
    }
}
