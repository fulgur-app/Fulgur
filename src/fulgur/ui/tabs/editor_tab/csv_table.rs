//! The table delegate backing the CSV grid view.

use gpui::{
    App, ClickEvent, Context, Entity, Focusable, InteractiveElement, IntoElement, ParentElement,
    SharedString, StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, WindowExt,
    button::ButtonVariant,
    dialog::DialogButtonProps,
    input::{Input, InputEvent, InputState},
    table::{Column, TableDelegate, TableState},
};

use crate::fulgur::files::csv_support::{CsvData, serialize_csv};

/// Fixed width of the synthetic row-number column.
const ROW_NUMBER_COLUMN_WIDTH: f32 = 52.0;
/// Approximate rendered width of a single character, used to size columns.
const CELL_CHARACTER_WIDTH: f32 = 7.5;
/// Horizontal padding added to a column's content width.
const CELL_PADDING_WIDTH: f32 = 24.0;
/// Minimum width a data column is allowed to shrink to.
const MIN_COLUMN_WIDTH: f32 = 60.0;
/// Maximum width a data column is allowed to grow to.
const MAX_COLUMN_WIDTH: f32 = 500.0;

/// Identifies which value a CSV edit dialog is targeting (data indices).
#[derive(Debug, Clone, Copy)]
enum EditTarget {
    /// The header of the data column at the given index.
    Header(usize),
    /// The cell at the given `(row, data column)` index.
    Cell(usize, usize),
}

/// A [`TableDelegate`] that renders and edits CSV data over a canonical buffer.
pub struct CsvTableDelegate {
    headers: Vec<String>,
    rows: Vec<Vec<String>>,
    columns: Vec<Column>,
    delimiter: u8,
    content: Entity<InputState>,
    dialog_input: Entity<InputState>,
}

impl CsvTableDelegate {
    /// Build a delegate from parsed CSV data.
    ///
    /// ### Arguments
    /// - `data`: The parsed headers and rows
    /// - `delimiter`: The delimiter to use when serializing back to text
    /// - `content`: The canonical `InputState` buffer to write edits back into
    /// - `dialog_input`: A reusable input entity for the edit dialog
    ///
    /// ### Returns
    /// - `CsvTableDelegate`: The constructed delegate
    pub fn new(
        data: CsvData,
        delimiter: u8,
        content: Entity<InputState>,
        dialog_input: Entity<InputState>,
    ) -> Self {
        let columns = Self::compute_columns(&data.headers, &data.rows);
        Self {
            headers: data.headers,
            rows: data.rows,
            columns,
            delimiter,
            content,
            dialog_input,
        }
    }

    /// Build the grid columns: a synthetic row-number column plus one column
    /// per header sized to its widest value (clamped to `MAX_COLUMN_WIDTH`).
    ///
    /// ### Arguments
    /// - `headers`: The header strings
    /// - `rows`: The data rows, used to measure content widths
    ///
    /// ### Returns
    /// - `Vec<Column>`: The row-number column followed by one column per header
    fn compute_columns(headers: &[String], rows: &[Vec<String>]) -> Vec<Column> {
        let mut columns = Vec::with_capacity(headers.len() + 1);
        columns.push(
            Column::new(
                SharedString::from("csv-row-number"),
                SharedString::default(),
            )
            .width(px(ROW_NUMBER_COLUMN_WIDTH))
            .resizable(false)
            .movable(false)
            .selectable(false)
            .text_center(),
        );

        for (index, header) in headers.iter().enumerate() {
            let mut max_chars = header.chars().count();
            for row in rows {
                if let Some(cell) = row.get(index) {
                    max_chars = max_chars.max(cell.chars().count());
                }
            }
            #[allow(clippy::cast_precision_loss)]
            let width = ((max_chars as f32) * CELL_CHARACTER_WIDTH + CELL_PADDING_WIDTH)
                .clamp(MIN_COLUMN_WIDTH, MAX_COLUMN_WIDTH);
            columns.push(
                Column::new(
                    SharedString::from(format!("csv-col-{index}")),
                    SharedString::from(header.clone()),
                )
                .width(px(width)),
            );
        }
        columns
    }

    /// Recompute the columns from the current headers and rows.
    fn refresh_columns(&mut self) {
        self.columns = Self::compute_columns(&self.headers, &self.rows);
    }

    /// Map a selected grid cell to a data column index, ignoring the synthetic
    /// row-number column.
    ///
    /// ### Arguments
    /// - `selected`: The selected `(row, grid column)`, if any
    ///
    /// ### Returns
    /// - `Some(usize)`: The data column index when a data column is selected
    /// - `None`: When nothing or only the row-number column is selected
    fn selected_data_column(selected: Option<(usize, usize)>) -> Option<usize> {
        selected.and_then(|(_, grid_col)| grid_col.checked_sub(1))
    }

    /// Serialize the current model back into the canonical text buffer.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    fn commit_to_buffer(&self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let text = serialize_csv(&self.headers, &self.rows, self.delimiter);
        self.content.update(cx, |state, cx| {
            state.set_value(text, window, cx);
            cx.emit(InputEvent::Change);
        });
    }

    /// Refresh columns, write the model back to the buffer, and notify.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    fn commit_and_refresh(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        self.refresh_columns();
        self.commit_to_buffer(window, cx);
        cx.notify();
    }

    /// Read the current value targeted by an edit dialog.
    ///
    /// ### Arguments
    /// - `target`: The header or cell to read (data indices)
    ///
    /// ### Returns
    /// - `String`: The current value, or an empty string if out of bounds
    fn value_at(&self, target: EditTarget) -> String {
        match target {
            EditTarget::Header(col) => self.headers.get(col).cloned().unwrap_or_default(),
            EditTarget::Cell(row, col) => self
                .rows
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
    fn open_edit_dialog(
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
        match target {
            EditTarget::Header(col) => {
                if let Some(header) = self.headers.get_mut(col) {
                    *header = value;
                }
            }
            EditTarget::Cell(row, col) => {
                if let Some(cell) = self.rows.get_mut(row).and_then(|cells| cells.get_mut(col)) {
                    *cell = value;
                }
            }
        }
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty row at `index`.
    ///
    /// ### Arguments
    /// - `index`: The row index to insert at
    /// - `window`: The active window
    /// - `cx`: The table state context
    fn insert_row_at(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let index = index.min(self.rows.len());
        self.rows
            .insert(index, vec![String::new(); self.headers.len()]);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty row above the selected row (or at the end if none).
    ///
    /// ### Arguments
    /// - `selected`: The currently selected `(row, grid column)`, if any
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_row_above(
        &mut self,
        selected: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let index = selected.map_or(self.rows.len(), |(row, _)| row);
        self.insert_row_at(index, window, cx);
    }

    /// Insert an empty row below the selected row (or at the end if none).
    ///
    /// ### Arguments
    /// - `selected`: The currently selected `(row, grid column)`, if any
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_row_below(
        &mut self,
        selected: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let index = selected.map_or(self.rows.len(), |(row, _)| row + 1);
        self.insert_row_at(index, window, cx);
    }

    /// Delete the selected row, or the last row if none is selected.
    ///
    /// ### Arguments
    /// - `selected`: The currently selected `(row, grid column)`, if any
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn delete_row(
        &mut self,
        selected: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        if self.rows.is_empty() {
            return;
        }
        let index = selected
            .map_or(self.rows.len() - 1, |(row, _)| row)
            .min(self.rows.len() - 1);
        self.rows.remove(index);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty data column at `index`.
    ///
    /// ### Arguments
    /// - `index`: The column index to insert at
    /// - `window`: The active window
    /// - `cx`: The table state context
    fn insert_column_at(
        &mut self,
        index: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let index = index.min(self.headers.len());
        self.headers.insert(index, String::new());
        for row in &mut self.rows {
            let position = index.min(row.len());
            row.insert(position, String::new());
        }
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column before the selected column (or at the end).
    ///
    /// ### Arguments
    /// - `selected`: The currently selected `(row, grid column)`, if any
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_column_before(
        &mut self,
        selected: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let index = Self::selected_data_column(selected).unwrap_or(self.headers.len());
        self.insert_column_at(index, window, cx);
    }

    /// Insert an empty column after the selected column (or at the end).
    ///
    /// ### Arguments
    /// - `selected`: The currently selected `(row, grid column)`, if any
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_column_after(
        &mut self,
        selected: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let index = Self::selected_data_column(selected).map_or(self.headers.len(), |col| col + 1);
        self.insert_column_at(index, window, cx);
    }

    /// Delete the selected column, or the last column if none is selected.
    ///
    /// ### Arguments
    /// - `selected`: The currently selected `(row, grid column)`, if any
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn delete_column(
        &mut self,
        selected: Option<(usize, usize)>,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        if self.headers.is_empty() {
            return;
        }
        let index = Self::selected_data_column(selected)
            .unwrap_or(self.headers.len() - 1)
            .min(self.headers.len() - 1);
        self.headers.remove(index);
        for row in &mut self.rows {
            if index < row.len() {
                row.remove(index);
            }
        }
        self.commit_and_refresh(window, cx);
    }
}

impl TableDelegate for CsvTableDelegate {
    fn columns_count(&self, _: &App) -> usize {
        self.headers.len() + 1
    }

    fn rows_count(&self, _: &App) -> usize {
        self.rows.len()
    }

    fn column(&self, col_ix: usize, _: &App) -> Column {
        self.columns.get(col_ix).cloned().unwrap_or_default()
    }

    fn render_th(
        &mut self,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let label = if col_ix == 0 {
            String::new()
        } else {
            self.headers.get(col_ix - 1).cloned().unwrap_or_default()
        };
        div()
            .id(SharedString::from(format!("csv-th-{col_ix}")))
            .size_full()
            .child(label)
            .when(col_ix != 0, |this| {
                let data_col = col_ix - 1;
                this.on_click(cx.listener(move |state, event: &ClickEvent, window, cx| {
                    if event.click_count() >= 2 {
                        state.delegate_mut().open_edit_dialog(
                            EditTarget::Header(data_col),
                            window,
                            cx,
                        );
                    }
                }))
            })
    }

    fn render_td(
        &mut self,
        row_ix: usize,
        col_ix: usize,
        _: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) -> impl IntoElement {
        let is_row_number = col_ix == 0;
        let muted = cx.theme().muted_foreground;
        let text = if is_row_number {
            (row_ix + 1).to_string()
        } else {
            self.rows
                .get(row_ix)
                .and_then(|cells| cells.get(col_ix - 1))
                .cloned()
                .unwrap_or_default()
        };
        div()
            .id(SharedString::from(format!("csv-td-{row_ix}-{col_ix}")))
            .size_full()
            .when(is_row_number, |this| {
                this.flex().justify_center().text_color(muted)
            })
            .child(text)
            .when(!is_row_number, |this| {
                let data_col = col_ix - 1;
                this.on_click(cx.listener(move |state, event: &ClickEvent, window, cx| {
                    if event.click_count() >= 2 {
                        state.delegate_mut().open_edit_dialog(
                            EditTarget::Cell(row_ix, data_col),
                            window,
                            cx,
                        );
                    }
                }))
            })
    }

    fn move_column(
        &mut self,
        col_ix: usize,
        to_ix: usize,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        // The row-number column (grid index 0) is not movable.
        let Some(from_data) = col_ix.checked_sub(1) else {
            return;
        };
        if from_data >= self.headers.len() {
            return;
        }
        let to_data = to_ix.saturating_sub(1).min(self.headers.len() - 1);
        if from_data == to_data {
            return;
        }

        let header = self.headers.remove(from_data);
        self.headers.insert(to_data, header);
        for row in &mut self.rows {
            if from_data < row.len() {
                let cell = row.remove(from_data);
                row.insert(to_data.min(row.len()), cell);
            }
        }
        self.commit_and_refresh(window, cx);
    }

    fn cell_text(&self, row_ix: usize, col_ix: usize, _: &App) -> String {
        if col_ix == 0 {
            return (row_ix + 1).to_string();
        }
        self.rows
            .get(row_ix)
            .and_then(|cells| cells.get(col_ix - 1))
            .cloned()
            .unwrap_or_default()
    }
}
