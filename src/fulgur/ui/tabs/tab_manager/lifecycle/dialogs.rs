use crate::fulgur::ui::dialogs::application_close::CloseScope;
use crate::fulgur::{Fulgur, PendingSaveCloseAction};
use gpui_kit::component::WindowExt;
use gpui_kit::{Context, ParentElement, Styled, Window, div, px};

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
                    let on_confirm_inner = on_confirm_clone.clone();
                    entity_ok.update(cx, |_this, cx| {
                        cx.defer_in(window, move |this, window, cx| {
                            on_confirm_inner(this, window, cx);
                            if !window.has_active_dialog(cx) {
                                this.focus_active_tab(window, cx);
                            }
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
                .close_button(false)
        });
    }

    /// Quit the application once every window's large-file warnings and the
    /// optional exit confirmation are resolved.
    ///
    /// ### Arguments
    /// - `window`: The window to quit the application in
    /// - `cx`: The application context
    pub fn quit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.defer_close_for_pending_saves(PendingSaveCloseAction::Quit) {
            return;
        }
        self.start_close_plan(CloseScope::Application, cx);
    }
}
