use super::WindowManager;
use crate::fulgur::Fulgur;
use crate::fulgur::ui::tabs::editor_tab::TabTransferData;
use gpui::*;
use gpui_component::notification::NotificationType;

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
}
