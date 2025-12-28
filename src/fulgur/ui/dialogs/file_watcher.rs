use std::{ops::DerefMut, path::PathBuf};

use gpui::*;
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, v_flex};

use crate::fulgur::Fulgur;

impl Fulgur {
    /// Show dialog when file is modified externally and has local changes
    ///
    /// ### Arguments
    /// - `path`: The path to the file that has local changes
    /// - `tab_index`: The index of the tab that has local changes
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_file_conflict_dialog(
        &self,
        path: PathBuf,
        tab_index: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity().clone();
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("file")
            .to_string();

        window.open_dialog(cx.deref_mut(), move |modal, _, _| {
            let entity_for_ok = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("File Modified Externally"))
                .keyboard(true)
                .confirm()
                .overlay_closable(false)
                .close_button(false)
                .button_props(
                    DialogButtonProps::default()
                        .cancel_text("Keep local changes")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Load from file")
                        .ok_variant(ButtonVariant::Primary),
                )
                .child(
                    v_flex()
                        .gap_2()
                        .child(format!(
                            "The file \"{}\" has been modified externally.",
                            filename
                        ))
                        .child("You have unsaved changes in this file. Do you want to load the changes from the file?"),
                )
                .on_ok(move |_, window, cx| {
                    entity_for_ok.update(cx, |this, cx| {
                        this.reload_tab_from_disk(tab_index, window, cx);
                    });
                    true
                })
                .on_cancel(|_, _, _| true)
        });
    }
}
