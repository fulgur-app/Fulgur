use std::ops::DerefMut;

use gpui::{Context, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps};

use crate::fulgur::{
    Fulgur,
    sync::ssh::session::{HostKeyDecision, HostKeyRequest},
};

impl Fulgur {
    /// Show the SSH host key fingerprint dialog for TOFU verification.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    /// - `request`: The host key verification request from the SSH background thread
    pub fn show_ssh_host_fingerprint_dialog(
        &self,
        window: &mut Window,
        cx: &mut Context<Self>,
        request: HostKeyRequest,
    ) {
        let host_label: SharedString = if request.port == 22 {
            format!("Host: {}", request.host).into()
        } else {
            format!("Host: {}:{}", request.host, request.port).into()
        };
        let fingerprint_label: SharedString = request.fingerprint.into();
        let tx = request.decision_tx;

        window.open_alert_dialog(cx.deref_mut(), move |modal, _, _| {
            let tx_ok = tx.clone();
            let tx_cancel = tx.clone();

            modal
                .title(div().text_size(px(16.)).child("Unknown SSH host"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Reject")
                        .cancel_variant(ButtonVariant::Danger)
                        .ok_text("Trust and connect")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(div().text_sm().child(host_label.clone()))
                .child(div().text_sm().mt_2().child("SHA-256 fingerprint:"))
                .child(
                    div()
                        .text_xs()
                        .font_family("monospace")
                        .mt_1()
                        .child(fingerprint_label.clone()),
                )
                .on_ok(move |_, _, _| {
                    let _ = tx_ok.send(HostKeyDecision::Accept);
                    true
                })
                .on_cancel(move |_, _, _| {
                    let _ = tx_cancel.send(HostKeyDecision::Reject);
                    true
                })
        });
    }
}
