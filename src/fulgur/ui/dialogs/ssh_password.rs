use std::cell::Cell;
use std::path::PathBuf;
use std::rc::Rc;
use std::sync::mpsc::Sender;

use gpui_kit::component::{
    ActiveTheme, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    v_flex,
};
use gpui_kit::{
    App, AppContext, Context, Entity, Focusable, ParentElement, PathPromptOptions, SharedString,
    Styled, Window, div, px,
};
use zeroize::Zeroizing;

use crate::fulgur::{
    Fulgur,
    sync::ssh::{
        auth::{CredentialRequest, CredentialRequestKind, LoginAnswer},
        session::{expand_tilde, home_dir},
    },
    ui::dialogs::ssh_challenge::SshChallenge,
};

impl Fulgur {
    /// Show the dialog matching a credential prompt posted by an SSH worker thread.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `request`: The prompt and the channel that unblocks the worker once answered
    pub fn show_ssh_credential_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        request: CredentialRequest,
    ) {
        let target = format_login_target(&request.user, &request.host, request.port);
        match request.kind {
            CredentialRequestKind::Passphrase {
                key_path,
                error,
                answer_tx,
            } => {
                self.show_ssh_passphrase_dialog(window, cx, &target, &key_path, error, answer_tx);
            }
            CredentialRequestKind::Login {
                password_allowed,
                error,
                answer_tx,
            } => {
                self.show_ssh_login_dialog(window, cx, &target, password_allowed, error, answer_tx);
            }
            CredentialRequestKind::Challenge {
                instructions,
                prompt,
                echo,
                answer_tx,
            } => {
                self.show_ssh_challenge_dialog(
                    window,
                    cx,
                    &target,
                    &SshChallenge {
                        instructions,
                        prompt,
                        echo,
                    },
                    answer_tx,
                );
            }
        }
    }

    /// Show the login dialog asking for a password or a private key file.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `target`: `user@host[:port]` label displayed in the title
    /// - `password_allowed`: Whether to show the password field
    /// - `error`: Message explaining why the previous attempt failed, if any
    /// - `answer_tx`: Channel receiving the password, the key file, or the cancellation
    pub fn show_ssh_login_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        target: &str,
        password_allowed: bool,
        error: Option<String>,
        answer_tx: Sender<LoginAnswer>,
    ) {
        let title: SharedString = format!("SSH login for {target}").into();
        let error: Option<SharedString> = error.map(SharedString::from);
        let hint: SharedString = if password_allowed {
            "When a key file is set, it is used instead of the password.".into()
        } else {
            "This server only accepts key authentication.".into()
        };
        let password_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Password")
                .masked(true)
        });
        let key_input =
            cx.new(|cx| InputState::new(window, cx).placeholder("Private key file (optional)"));
        let has_initialized_focus = Rc::new(Cell::new(false));

        window.open_alert_dialog(cx, move |modal, window, cx| {
            if !has_initialized_focus.get() {
                let initial_input = if password_allowed {
                    &password_input
                } else {
                    &key_input
                };
                let focus_handle = initial_input.read(cx).focus_handle(cx);
                window.focus(&focus_handle, cx);
                has_initialized_focus.set(true);
            }

            let password_input_ok = password_input.clone();
            let key_input_ok = key_input.clone();
            let key_input_browse = key_input.clone();
            let answer_ok = answer_tx.clone();
            let answer_cancel = answer_tx.clone();

            let mut form = v_flex().w_full().gap_2();
            if let Some(error) = &error {
                form = form.child(
                    div()
                        .text_sm()
                        .text_color(cx.theme().danger)
                        .child(error.clone()),
                );
            }
            if password_allowed {
                form = form.child(Input::new(&password_input));
            }
            let form = form
                .child(
                    h_flex()
                        .w_full()
                        .gap_2()
                        .child(div().flex_1().child(Input::new(&key_input)))
                        .child(
                            Button::new("ssh-login-browse-key")
                                .label("Browse...")
                                .with_variant(ButtonVariant::Secondary)
                                .on_click(move |_, window, cx| {
                                    browse_for_key_file(&key_input_browse, window, cx);
                                }),
                        ),
                )
                .child(
                    div()
                        .text_xs()
                        .text_color(cx.theme().muted_foreground)
                        .child(hint.clone()),
                );

            modal
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
                .close_button(false)
                .child(form)
                .on_ok(move |_, window: &mut Window, cx| {
                    let key_path = key_input_ok.read(cx).value().trim().to_string();
                    let answer = if key_path.is_empty() {
                        let password =
                            Zeroizing::new(password_input_ok.read(cx).value().to_string());
                        if password.is_empty() {
                            let message = if password_allowed {
                                "Enter a password or choose a key file"
                            } else {
                                "Choose a key file"
                            };
                            window.push_notification(
                                (NotificationType::Error, SharedString::from(message)),
                                cx,
                            );
                            return false;
                        }
                        LoginAnswer::Password(password)
                    } else {
                        LoginAnswer::KeyFile(expand_key_path(&key_path))
                    };
                    let _ = answer_ok.send(answer);
                    true
                })
                .on_cancel(move |_, _, _| {
                    let _ = answer_cancel.send(LoginAnswer::Cancel);
                    true
                })
        });
    }
}

/// Open the system file picker and put the chosen key path into the key file input.
///
/// ### Arguments
/// - `key_input`: Input receiving the selected path
/// - `window`: The window owning the dialog
/// - `cx`: The application context
fn browse_for_key_file(key_input: &Entity<InputState>, window: &mut Window, cx: &mut App) {
    let selected_paths = cx.prompt_for_paths(PathPromptOptions {
        files: true,
        directories: false,
        multiple: false,
        prompt: Some("Select private key".into()),
    });
    let key_input = key_input.clone();
    window
        .spawn(cx, async move |cx| {
            let path = selected_paths.await.ok()?.ok()??.into_iter().next()?;
            cx.update(|window, cx| {
                key_input.update(cx, |state, cx| {
                    state.set_value(path.display().to_string(), window, cx);
                });
            })
            .ok()
        })
        .detach();
}

/// Expand a leading `~` in a key path typed by the user.
///
/// ### Arguments
/// - `raw`: Path as typed
///
/// ### Returns
/// - `PathBuf`: The path with `~` expanded, or unchanged when the home directory is unknown
fn expand_key_path(raw: &str) -> PathBuf {
    home_dir().map_or_else(|_| PathBuf::from(raw), |home| expand_tilde(raw, &home))
}

/// Format the `user@host[:port]` label used by SSH credential dialogs.
///
/// ### Arguments
/// - `user`: Remote username
/// - `host`: Remote hostname
/// - `port`: SSH port, omitted when it is the default 22
///
/// ### Returns
/// - `String`: The formatted label
pub(super) fn format_login_target(user: &str, host: &str, port: u16) -> String {
    if port == 22 {
        format!("{user}@{host}")
    } else {
        format!("{user}@{host}:{port}")
    }
}

#[cfg(test)]
mod tests {
    use super::format_login_target;

    #[test]
    fn format_login_target_omits_default_port() {
        assert_eq!(
            format_login_target("alice", "example.com", 22),
            "alice@example.com"
        );
        assert_eq!(
            format_login_target("alice", "example.com", 2222),
            "alice@example.com:2222"
        );
    }
}
