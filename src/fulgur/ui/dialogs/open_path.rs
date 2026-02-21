use std::ops::DerefMut;
use std::path::PathBuf;

use gpui::*;
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, notification::NotificationType,
};

use super::path_browser::PathBrowser;
use crate::fulgur::Fulgur;

impl Fulgur {
    pub fn show_open_from_path_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity().clone();
        let path_browser = cx.new(|cx| PathBrowser::new(window, cx));
        let input = path_browser.read(cx).input().clone();
        let input_clone = input.clone();
        window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            let entity_ok = entity.clone();
            let input_ok = input_clone.clone();
            let path_browser = path_browser.clone();
            modal
                .title(div().text_size(px(16.)).child("Open file from path..."))
                .keyboard(true)
                .confirm()
                .overlay_closable(false)
                .close_button(false)
                .button_props(
                    DialogButtonProps::default()
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Open")
                        .ok_variant(ButtonVariant::Primary),
                )
                .child(path_browser)
                .on_ok(move |_, window, cx| {
                    let path_str = input_ok.read(cx).value().trim().to_string();
                    if path_str.is_empty() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from("Please enter a file path"),
                            ),
                            cx,
                        );
                        return false;
                    }
                    let path = PathBuf::from(&path_str);
                    if !path.exists() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from(format!("Path does not exist: {}", path_str)),
                            ),
                            cx,
                        );
                        return false;
                    }
                    if !path.is_file() {
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from(format!("Path is not a file: {}", path_str)),
                            ),
                            cx,
                        );
                        return false;
                    }
                    entity_ok.update(cx, |this, cx| {
                        this.do_open_file(window, cx, path);
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}
