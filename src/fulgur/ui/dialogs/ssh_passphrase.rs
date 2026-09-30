use std::cell::Cell;
use std::path::Path;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    input::{Input, InputState},
    notification::NotificationType,
    v_flex,
};
use gpui_kit::{
    AppContext, Context, Focusable, ParentElement, SharedString, Styled, Window, div, px,
};
use zeroize::Zeroizing;

use crate::fulgur::{
    Fulgur,
    sync::ssh::{auth::PassphraseAnswer, session::home_dir},
};

impl Fulgur {
    /// Show the dialog asking for the passphrase of an encrypted private key.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `target`: `user@host[:port]` label displayed in the prompt
    /// - `key_path`: Private key file to unlock
    /// - `error`: Message explaining why the previous attempt failed, if any
    /// - `answer_tx`: Channel receiving the passphrase, or the decision to skip the key
    pub fn show_ssh_passphrase_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        target: &str,
        key_path: &Path,
        error: Option<String>,
        answer_tx: Sender<PassphraseAnswer>,
    ) {
        let prompt: SharedString = format!(
            "Enter the passphrase of {} to log in as {target}.",
            display_key_path(key_path)
        )
        .into();
        let error: Option<SharedString> = error.map(SharedString::from);
        let passphrase_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Passphrase")
                .masked(true)
        });
        let has_initialized_focus = Rc::new(Cell::new(false));

        window.open_alert_dialog(cx, move |modal, window, cx| {
            if !has_initialized_focus.get() {
                let focus_handle = passphrase_input.read(cx).focus_handle(cx);
                window.focus(&focus_handle, cx);
                has_initialized_focus.set(true);
            }

            let passphrase_input_ok = passphrase_input.clone();
            let answer_ok = answer_tx.clone();
            let answer_skip = answer_tx.clone();

            let mut form = v_flex()
                .w_full()
                .gap_2()
                .child(div().text_sm().child(prompt.clone()));
            if let Some(error) = &error {
                form = form.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                );
            }
            let form = form.child(Input::new(&passphrase_input));

            modal
                .title(div().text_size(px(16.)).child("Unlock SSH key"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Skip key")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Unlock")
                        .ok_variant(ButtonVariant::Primary),
                )
                .close_button(false)
                .child(form)
                .on_ok(move |_, window: &mut Window, cx| {
                    let passphrase =
                        Zeroizing::new(passphrase_input_ok.read(cx).value().to_string());
                    if passphrase.is_empty() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from("Passphrase is required"),
                            ),
                            cx,
                        );
                        return false;
                    }
                    let _ = answer_ok.send(PassphraseAnswer::Passphrase(passphrase));
                    true
                })
                .on_cancel(move |_, _, _| {
                    let _ = answer_skip.send(PassphraseAnswer::Skip);
                    true
                })
        });
    }
}

/// Shorten a key path under the home directory to its `~/...` form for display.
///
/// ### Arguments
/// - `key_path`: Private key file
///
/// ### Returns
/// - `String`: `~/relative/path` under the home directory, the full path otherwise
fn display_key_path(key_path: &Path) -> String {
    home_dir()
        .ok()
        .and_then(|home| key_path.strip_prefix(home).ok().map(Path::to_path_buf))
        .map_or_else(
            || key_path.display().to_string(),
            |relative| Path::new("~").join(relative).display().to_string(),
        )
}
