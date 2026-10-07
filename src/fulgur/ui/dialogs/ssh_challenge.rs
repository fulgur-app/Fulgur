use std::cell::Cell;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    input::{Input, InputState},
    v_flex,
};
use gpui_kit::{
    AppContext, Context, Focusable, ParentElement, SharedString, Styled, Window, div, px,
};

use crate::fulgur::{
    Fulgur, sync::ssh::auth::ChallengeAnswer, ui::dialogs::ssh_password::read_secret,
};

/// A keyboard-interactive question sent by the SSH server.
pub struct SshChallenge {
    /// Informational text sent by the server; may be empty.
    pub instructions: String,
    /// The question, as worded by the server.
    pub prompt: String,
    /// Whether the answer may be displayed while typed.
    pub echo: bool,
}

impl Fulgur {
    /// Show the dialog answering a keyboard-interactive question, e.g. a one-time code.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `target`: `user@host[:port]` label displayed in the title
    /// - `challenge`: The server's question
    /// - `answer_tx`: Channel receiving the response or the cancellation
    pub fn show_ssh_challenge_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        target: &str,
        challenge: &SshChallenge,
        answer_tx: Sender<ChallengeAnswer>,
    ) {
        let title: SharedString = format!("SSH login for {target}").into();
        let instructions: Option<SharedString> = Some(challenge.instructions.trim())
            .filter(|text| !text.is_empty())
            .map(|text| SharedString::from(text.to_string()));
        let prompt: SharedString = challenge.prompt.trim().to_string().into();
        let response_input = cx.new(|cx| InputState::new(window, cx).masked(!challenge.echo));
        let has_initialized_focus = Rc::new(Cell::new(false));

        window.open_alert_dialog(cx, move |modal, window, cx| {
            if !has_initialized_focus.get() {
                let focus_handle = response_input.read(cx).focus_handle(cx);
                window.focus(&focus_handle, cx);
                has_initialized_focus.set(true);
            }

            let response_input_ok = response_input.clone();
            let answer_ok = answer_tx.clone();
            let answer_cancel = answer_tx.clone();

            let mut form = v_flex().w_full().gap_2();
            if let Some(instructions) = &instructions {
                form = form.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child(instructions.clone()),
                );
            }
            let form = form
                .child(div().text_sm().child(prompt.clone()))
                .child(Input::new(&response_input));

            modal
                .title(div().text_size(px(16.)).child(title.clone()))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Submit")
                        .ok_variant(ButtonVariant::Primary),
                )
                .close_button(false)
                .child(form)
                .on_ok(move |_, _, cx| {
                    let response = read_secret(response_input_ok.read(cx).text());
                    let _ = answer_ok.send(ChallengeAnswer::Response(response));
                    true
                })
                .on_cancel(move |_, _, _| {
                    let _ = answer_cancel.send(ChallengeAnswer::Cancel);
                    true
                })
        });
    }
}
