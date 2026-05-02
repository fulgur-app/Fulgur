use std::cell::Cell;
use std::ops::DerefMut;
use std::rc::Rc;
use std::sync::Arc;

use gpui::{
    App, AppContext, Context, Focusable, ParentElement, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input,
    notification::NotificationType, v_flex,
};
use zeroize::Zeroizing;

use crate::fulgur::Fulgur;

impl Fulgur {
    /// Shows a modal with an optional username field (when `user` is `None`) and a masked
    /// password field. Calls `on_confirm` with the resolved username and password on submit.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `host`: Remote hostname displayed in the dialog title
    /// - `port`: SSH port (appended to title only when not 22)
    /// - `user`: Pre-filled username, or `None` to show an editable username field
    /// - `on_confirm`: Callback invoked with `(username, password, window, cx)` on OK
    pub fn show_ssh_password_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        host: &str,
        port: u16,
        user: Option<String>,
        on_confirm: impl Fn(String, Zeroizing<String>, &mut Window, &mut App) + 'static,
    ) {
        let title: SharedString = if port == 22 {
            format!("SSH login for {host}").into()
        } else {
            format!("SSH login for {host}:{port}").into()
        };
        let show_user_field = user.is_none();
        let prefilled_user = user.unwrap_or_default();

        let user_input =
            cx.new(|cx| gpui_component::input::InputState::new(window, cx).placeholder("Username"));
        let password_input = cx.new(|cx| {
            gpui_component::input::InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });

        let user_input_ok = user_input.clone();
        let password_input_ok = password_input.clone();
        let prefilled_ok = prefilled_user.clone();
        let on_confirm = Arc::new(on_confirm);
        let has_initialized_focus = Rc::new(Cell::new(false));

        window.open_alert_dialog(cx.deref_mut(), move |modal, window, cx| {
            if !has_initialized_focus.get() {
                let focus_handle = if show_user_field {
                    user_input.read(cx).focus_handle(cx)
                } else {
                    password_input.read(cx).focus_handle(cx)
                };
                window.focus(&focus_handle, cx);
                has_initialized_focus.set(true);
            }

            let user_input_inner = user_input_ok.clone();
            let password_input_inner = password_input_ok.clone();
            let prefilled_inner = prefilled_ok.clone();
            let on_confirm_inner = Arc::clone(&on_confirm);

            let m = modal
                .title(div().text_size(px(16.)).child(title.clone()))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Connect")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false);

            let form = if show_user_field {
                v_flex()
                    .w_full()
                    .gap_2()
                    .child(Input::new(&user_input))
                    .child(Input::new(&password_input))
            } else {
                v_flex().w_full().gap_2().child(Input::new(&password_input))
            };

            m.child(form)
                .on_ok(move |_, window: &mut Window, cx| {
                    let username = if show_user_field {
                        user_input_inner.read(cx).value().to_string()
                    } else {
                        prefilled_inner.clone()
                    };
                    if username.trim().is_empty() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from("Username is required"),
                            ),
                            cx,
                        );
                        return false;
                    }
                    let password_str = password_input_inner.read(cx).value().to_string();
                    if password_str.is_empty() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from("Password is required"),
                            ),
                            cx,
                        );
                        return false;
                    }
                    on_confirm_inner(username, Zeroizing::new(password_str), window, cx);
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}
