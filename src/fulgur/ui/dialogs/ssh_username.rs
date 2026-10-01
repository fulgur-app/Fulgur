use std::cell::Cell;
use std::rc::Rc;
use std::sync::Arc;

use gpui_kit::component::{
    WindowExt,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    input::{Input, InputState},
    notification::NotificationType,
};
use gpui_kit::{
    App, AppContext, Context, Focusable, ParentElement, SharedString, Styled, Window, div, px,
};

use crate::fulgur::Fulgur;

impl Fulgur {
    /// Show the dialog asking for the SSH username when the remote URL has none.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `host`: Remote hostname displayed in the dialog title
    /// - `port`: SSH port (appended to the title only when not 22)
    /// - `on_confirm`: Callback invoked with `(username, window, cx)` on submit
    pub fn show_ssh_username_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        host: &str,
        port: u16,
        on_confirm: impl Fn(String, &mut Window, &mut App) + 'static,
    ) {
        let title: SharedString = if port == 22 {
            format!("SSH login for {host}").into()
        } else {
            format!("SSH login for {host}:{port}").into()
        };
        let user_input = cx.new(|cx| InputState::new(window, cx).placeholder("Username"));
        let on_confirm = Arc::new(on_confirm);
        let has_initialized_focus = Rc::new(Cell::new(false));

        window.open_alert_dialog(cx, move |modal, window, cx| {
            if !has_initialized_focus.get() {
                let focus_handle = user_input.read(cx).focus_handle(cx);
                window.focus(&focus_handle, cx);
                has_initialized_focus.set(true);
            }

            let user_input_ok = user_input.clone();
            let on_confirm_ok = Arc::clone(&on_confirm);

            modal
                .title(div().text_size(px(16.)).child(title.clone()))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Continue")
                        .ok_variant(ButtonVariant::Primary),
                )
                .close_button(false)
                .child(Input::new(&user_input))
                .on_ok(move |_, window: &mut Window, cx| {
                    let username = user_input_ok.read(cx).value().trim().to_string();
                    if username.is_empty() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from("Username is required"),
                            ),
                            cx,
                        );
                        return false;
                    }
                    on_confirm_ok(username, window, cx);
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}
