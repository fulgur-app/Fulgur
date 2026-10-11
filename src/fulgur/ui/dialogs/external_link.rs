use gpui_kit::component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, v_flex};
use gpui_kit::{App, ParentElement, SharedString, Styled, Window, div, px};

/// Ask before handing a Markdown preview link with an untrusted scheme to the
/// system, since it may launch an arbitrary OS or third-party protocol handler.
///
/// ### Arguments
/// - `url`: The full link target, shown verbatim so the user sees what will run
/// - `window`: The window to show the dialog in
/// - `cx`: The application context
pub fn show_external_link_dialog(url: String, window: &mut Window, cx: &mut App) {
    let url_label: SharedString = url.clone().into();
    window.open_alert_dialog(cx, move |modal, _, _| {
        let url = url.clone();
        modal
            .title(div().text_size(px(16.)).child("Open External Link?"))
            .keyboard(true)
            .button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .cancel_text("Cancel")
                    .cancel_variant(ButtonVariant::Secondary)
                    .ok_text("Open")
                    .ok_variant(ButtonVariant::Danger),
            )
            .close_button(false)
            .child(
                v_flex()
                    .gap_2()
                    .child(
                        "This link is not a web address and will be handed to another \
                         application on your system. Only open it if you trust the document.",
                    )
                    .child(
                        div()
                            .text_xs()
                            .font_family("monospace")
                            .child(url_label.clone()),
                    ),
            )
            .on_ok(move |_, _, cx| {
                log::debug!("Opening confirmed markdown preview link {url}");
                cx.open_url(&url);
                true
            })
            .on_cancel(|_, _, _| true)
    });
}
