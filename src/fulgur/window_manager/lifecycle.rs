use super::WindowManager;
use crate::fulgur::WindowInit;
use crate::fulgur::ui::dialogs::large_file_close::CloseContinuation;
use crate::fulgur::ui::tabs::editor_tab::{TabLocation, TabTransferData};
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{Fulgur, PendingSaveCloseAction};
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::NotificationType;
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
        if self.defer_close_for_pending_saves(PendingSaveCloseAction::Window) {
            return false;
        }
        let window_count = cx.global::<WindowManager>().window_count();
        // A closed non-last window cannot be reopened, so its persisted buffers
        // would be lost; confirm each one like closing the tab would.
        if window_count > 1 && self.settings.app_settings.persist_unsaved_buffers {
            let unsaved = self.tabs_needing_window_close_prompt(cx);
            if !unsaved.is_empty() {
                self.drive_window_close_unsaved_prompts(unsaved, window, cx);
                return false;
            }
        }
        // Large, modified local files cannot be persisted, so warn about each before closing.
        let large_modified = self.large_modified_local_tabs(cx);
        if !large_modified.is_empty() {
            let continuation = if window_count == 1 {
                CloseContinuation::Quit
            } else {
                CloseContinuation::CloseWindow
            };
            self.drive_large_file_close_warnings(large_modified, continuation, window, cx);
            return false;
        }
        if window_count == 1 {
            if self.settings.app_settings.confirm_exit {
                self.quit(window, cx);
                false
            } else {
                if let Err(e) = self.save_state(cx, window) {
                    log::error!("Failed to save app state on window close: {e}");
                    if self.save_failed_once {
                        log::warn!("Save failed again - allowing force-close");
                    } else {
                        self.save_failed_once = true;
                        window.push_notification(
                            (
                                NotificationType::Error,
                                gpui_kit::SharedString::from(format!(
                                    "Failed to save application state: {e}. Close again to force-close."
                                )),
                            ),
                            cx,
                        );
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
            if let Err(e) = self.save_state_without_this_window(cx) {
                log::error!("Failed to save app state on window close: {e}");
                if self.save_failed_once {
                    log::warn!("Save failed again, allowing force-close");
                } else {
                    self.save_failed_once = true;
                    window.push_notification(
                        (
                            NotificationType::Error,
                            gpui_kit::SharedString::from(format!(
                                "Failed to save application state: {e}. Close again to force-close."
                            )),
                        ),
                        cx,
                    );
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

    /// Finish closing this window programmatically after close warnings.
    ///
    /// ### Arguments
    /// - `window`: The window being closed
    /// - `cx`: The application context
    pub(crate) fn finish_window_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        // Another window may have closed while the warnings were open; the last
        // window keeps its session so it is restored on the next launch.
        let is_last_window = cx.global::<WindowManager>().window_count() <= 1;
        let save_result = if is_last_window {
            self.save_state(cx, window)
        } else {
            self.save_state_without_this_window(cx)
        };
        if let Err(e) = save_result {
            log::error!("Failed to save app state on window close: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    gpui_kit::SharedString::from(format!("Failed to save application state: {e}.")),
                ),
                cx,
            );
            return;
        }
        cx.update_global::<WindowManager, _>(|manager, _| {
            manager.unregister(self.window_id);
        });
        for weak in cx.global::<WindowManager>().get_all_windows() {
            if let Some(entity) = weak.upgrade() {
                entity.update(cx, |_, cx| cx.notify());
            }
        }
        window.remove_window();
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
                let has_large_file_warning = matches!(editor_tab.location, TabLocation::Local(_))
                    && editor_tab.content_too_large_to_persist(cx)
                    && editor_tab.content_differs_from_original(cx);
                Some(editor_tab.modified && !has_large_file_warning)
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
            let large_modified = self.large_modified_local_tabs(cx);
            self.drive_large_file_close_warnings(
                large_modified,
                CloseContinuation::CloseWindow,
                window,
                cx,
            );
            return;
        };
        let rest: Vec<TabId> = queue.collect();
        if let Some(index) = self.tab_index_of(tab_id, cx) {
            self.set_active_tab(index, window, cx);
        }
        self.show_unsaved_changes_dialog(window, cx, move |_, window, cx| {
            let rest = rest.clone();
            // The confirmed dialog closes after this callback returns, and closing
            // pops the topmost dialog, so the next prompt must open afterwards.
            cx.defer_in(window, move |this, window, cx| {
                this.drive_window_close_unsaved_prompts(rest, window, cx);
            });
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
