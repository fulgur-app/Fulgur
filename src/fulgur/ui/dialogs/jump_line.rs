use std::ops::DerefMut;

use gpui::*;
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input};

use crate::fulgur::{Fulgur, editor_tab};

impl Fulgur {
    /// Show the jump to line dialog
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_jump_to_line_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_to_line_input.update(cx, |input_state, cx| {
            input_state.set_value("", window, cx);
            cx.notify();
        });
        let jump_to_line_input = self.jump_to_line_input.clone();
        let entity = cx.entity().clone();
        self.jump_to_line_dialog_open = true;
        window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = jump_to_line_input.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            let jump_to_line_input_clone = jump_to_line_input.clone();
            let entity_for_ok = entity.clone();
            let entity_for_cancel = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("Jump to line..."))
                .keyboard(true)
                .confirm()
                .overlay_closable(true)
                .close_button(false)
                .button_props(
                    DialogButtonProps::default()
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Jump")
                        .ok_variant(ButtonVariant::Primary),
                )
                .child(Input::new(&jump_to_line_input))
                .on_ok(move |_, _, cx| {
                    let text = jump_to_line_input_clone.read(cx).value();
                    let jump_result = editor_tab::extract_line_number(text);
                    let is_ok = jump_result.is_ok();
                    entity_for_ok.update(cx, |this, cx| {
                        if let Ok(jump) = jump_result {
                            this.pending_jump = Some(jump);
                            this.jump_to_line_dialog_open = false;
                            cx.notify();
                        } else {
                            this.pending_jump = None;
                        }
                    });

                    is_ok
                })
                .on_cancel(move |_, _, cx| {
                    entity_for_cancel.update(cx, |this, cx| {
                        this.jump_to_line_dialog_open = false;
                        cx.notify();
                    });
                    true
                })
        });
    }
}
