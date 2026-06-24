//! Single header/cell editing and the modal edit dialog.

use gpui::{Context, Focusable, ParentElement, Styled, Window, div, px};
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input, table::TableState,
};

use super::{CsvTableDelegate, EditTarget};

impl CsvTableDelegate {
    /// Read the current value targeted by an edit dialog.
    ///
    /// ### Arguments
    /// - `target`: The header or cell to read (data indices)
    ///
    /// ### Returns
    /// - `String`: The current value, or an empty string if out of bounds
    fn value_at(&self, target: EditTarget) -> String {
        Self::value_at_in(&self.headers, &self.rows, target)
    }

    /// Read the value at a header or cell from a plain headers/rows model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers
    /// - `rows`: The data rows
    /// - `target`: The header or cell to read (data indices)
    ///
    /// ### Returns
    /// - `String`: The current value, or an empty string if out of bounds
    pub(super) fn value_at_in(
        headers: &[String],
        rows: &[Vec<String>],
        target: EditTarget,
    ) -> String {
        match target {
            EditTarget::Header(col) => headers.get(col).cloned().unwrap_or_default(),
            EditTarget::Cell(row, col) => rows
                .get(row)
                .and_then(|cells| cells.get(col))
                .cloned()
                .unwrap_or_default(),
        }
    }

    /// Open the modal edit dialog for a header or cell.
    ///
    /// ### Arguments
    /// - `target`: The header or cell being edited (data indices)
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub(super) fn open_edit_dialog(
        &self,
        target: EditTarget,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let current = self.value_at(target);
        let input = self.dialog_input.clone();
        input.update(cx, |state, cx| {
            state.set_value(current, window, cx);
            cx.notify();
        });

        let table = cx.entity();
        let title = match target {
            EditTarget::Header(_) => "Edit column header",
            EditTarget::Cell(_, _) => "Edit cell",
        };

        window.open_alert_dialog(cx, move |dialog, window, cx| {
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            let input_for_ok = input.clone();
            let table_for_ok = table.clone();
            dialog
                .title(div().text_size(px(16.)).child(title))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("OK")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(true)
                .close_button(false)
                .child(Input::new(&input))
                .on_ok(move |_, window, cx| {
                    let value = input_for_ok.read(cx).value().to_string();
                    table_for_ok.update(cx, |state, cx| {
                        state.delegate_mut().apply_edit(target, value, window, cx);
                    });
                    true
                })
        });
    }

    /// Write an edited value back into the model and the canonical buffer.
    ///
    /// ### Arguments
    /// - `target`: The header or cell being edited (data indices)
    /// - `value`: The new value
    /// - `window`: The active window
    /// - `cx`: The table state context
    fn apply_edit(
        &mut self,
        target: EditTarget,
        value: String,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        Self::apply_edit_in(&mut self.headers, &mut self.rows, target, value);
        self.commit_and_refresh(window, cx);
    }

    /// Write an edited value into a plain headers/rows model, ignoring
    /// out-of-bounds targets.
    ///
    /// ### Arguments
    /// - `headers`: The column headers
    /// - `rows`: The data rows
    /// - `target`: The header or cell being edited (data indices)
    /// - `value`: The new value
    pub(super) fn apply_edit_in(
        headers: &mut [String],
        rows: &mut [Vec<String>],
        target: EditTarget,
        value: String,
    ) {
        match target {
            EditTarget::Header(col) => {
                if let Some(header) = headers.get_mut(col) {
                    *header = value;
                }
            }
            EditTarget::Cell(row, col) => {
                if let Some(cell) = rows.get_mut(row).and_then(|cells| cells.get_mut(col)) {
                    *cell = value;
                }
            }
        }
    }
}
