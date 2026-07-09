use crate::fulgur::Fulgur;
use gpui::{App, Context, ParentElement, Styled, Window, div, px};
use gpui_component::WindowExt;

impl Fulgur {
    /// Show confirmation dialog for unsaved changes
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `on_confirm`: Callback executed when user confirms closing
    pub fn show_unsaved_changes_dialog<F>(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        on_confirm: F,
    ) where
        F: Fn(&mut Fulgur, &mut Window, &mut Context<Fulgur>) + 'static + Clone,
    {
        let entity = cx.entity().clone();
        window.open_alert_dialog(cx, move |modal, _, _| {
            let entity_ok = entity.clone();
            let on_confirm_clone = on_confirm.clone();
            modal
                .title(div().text_size(px(16.)).child("Unsaved changes"))
                .keyboard(true)
                .show_cancel(true)
                .on_ok(move |_, window, cx| {
                    let entity_ok_footer = entity_ok.clone();
                    let on_confirm_inner = on_confirm_clone.clone();
                    entity_ok_footer.update(cx, |this, cx| {
                        on_confirm_inner(this, window, cx);
                    });
                    entity_ok_footer.update(cx, |_this, cx| {
                        cx.defer_in(window, move |this, window, cx| {
                            this.focus_active_tab(window, cx);
                        });
                    });
                    true
                })
                .on_cancel(move |_, _window, _cx| true)
                .child(
                    div().text_size(px(14.)).child(
                        "Are you sure you want to close this tab? Your changes will be lost.",
                    ),
                )
                .overlay_closable(false)
                .close_button(false)
        });
    }

    /// Quit the application. If `confirm_exit` is enabled, a modal will be shown to confirm the action.
    ///
    /// ### Arguments
    /// - `window`: The window to quit the application in
    /// - `cx`: The application context
    pub fn quit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.settings.app_settings.confirm_exit {
            let entity = cx.entity().clone();
            window.open_alert_dialog(cx, move |modal, _, _| {
                let entity_ok = entity.clone();
                modal
                    .title(div().text_size(px(16.)).child("Quit Fulgur"))
                    .keyboard(true)
                    .show_cancel(true)
                    .on_ok(move |_, window: &mut Window, cx: &mut App| {
                        let entity_ok_footer = entity_ok.clone();
                        let save_result =
                            entity_ok_footer.update(cx, |this, cx| this.save_state(cx, window));
                        if let Err(e) = save_result {
                            log::error!("Failed to save app state on quit: {e}");
                            window.push_notification(
                                (
                                    gpui_component::notification::NotificationType::Error,
                                    gpui::SharedString::from(format!(
                                        "Failed to save application state: {e}. Quit anyway?"
                                    )),
                                ),
                                cx,
                            );
                            return false; // Don't quit, show notification
                        }
                        cx.quit();
                        true
                    })
                    .on_cancel(move |_, _window, _cx| true)
                    .child(
                        div()
                            .text_size(px(14.))
                            .child("Are you sure you want to quit Fulgur?"),
                    )
                    .overlay_closable(false)
                    .close_button(false)
            });
            return;
        }
        if let Err(e) = self.save_state(cx, window) {
            log::error!("Failed to save app state on quit: {e}");
            window.push_notification(
                (
                    gpui_component::notification::NotificationType::Error,
                    gpui::SharedString::from(format!(
                        "Failed to save application state: {e}. Try again or close the app to quit without saving."
                    )),
                ),
                cx,
            );
            return; // Don't quit, show notification and let user try again
        }
        cx.quit();
    }
}
