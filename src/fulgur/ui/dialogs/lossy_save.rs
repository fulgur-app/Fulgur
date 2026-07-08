use crate::fulgur::ui::tabs::tab::TabId;
use std::path::PathBuf;

use gpui::{Context, ParentElement, Styled, Window, div, px};
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, v_flex};

use crate::fulgur::{Fulgur, tab::Tab, ui::components_utils::UTF_8};

impl Fulgur {
    /// Show a confirmation dialog when saving would lose data through encoding.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab being saved
    /// - `encoding`: The current encoding label that cannot represent the text
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_lossy_save_dialog(
        &self,
        tab_id: TabId,
        encoding: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let encoding = encoding.to_string();
        window.open_alert_dialog(cx, move |modal, _, _| {
            let entity_for_ok = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("Cannot Save in Encoding"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Save as UTF-8")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "This file contains characters that cannot be represented in {encoding}."
                        ))
                        .child("Save it as UTF-8 instead? This changes the file's encoding."),
                )
                .on_ok(move |_, window, cx| {
                    entity_for_ok.update(cx, |this, cx| {
                        if let Some(Tab::Editor(editor_tab)) =
                            this.tabs.iter_mut().find(|tab| tab.id() == tab_id)
                        {
                            editor_tab.encoding = UTF_8.to_string();
                            editor_tab.lossy_decode = false;
                        }
                        this.save_file(window, cx);
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }

    /// Show a confirmation dialog when "Save as" would lose data through encoding.
    ///
    /// ### Arguments
    /// - `tab_id`: Stable id of the tab being saved
    /// - `path`: The chosen destination path
    /// - `contents`: The editor text to write as UTF-8 on confirm
    /// - `encoding`: The current encoding label that cannot represent the text
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_lossy_save_as_dialog(
        &self,
        tab_id: TabId,
        path: PathBuf,
        contents: String,
        encoding: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let encoding = encoding.to_string();
        window.open_alert_dialog(cx, move |modal, _, _| {
            let entity_for_ok = entity.clone();
            let path = path.clone();
            let contents = contents.clone();
            modal
                .title(div().text_size(px(16.)).child("Cannot Save in Encoding"))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Save as UTF-8")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "This file contains characters that cannot be represented in {encoding}."
                        ))
                        .child("Save it as UTF-8 instead? This changes the file's encoding."),
                )
                .on_ok(move |_, window, cx| {
                    entity_for_ok.update(cx, |this, cx| {
                        this.finalize_save_as(
                            tab_id,
                            &path,
                            contents.as_bytes(),
                            UTF_8.to_string(),
                            window,
                            cx,
                        );
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}
