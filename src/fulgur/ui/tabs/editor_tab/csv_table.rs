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
        let text = match serialize_csv(&self.headers, &self.rows, self.delimiter) {
            Ok(text) => text,
            Err(error) => {
                log::error!("Refusing to commit CSV edit, serialization failed: {error}");
                return;
            }
        };
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
    fn value_at_in(headers: &[String], rows: &[Vec<String>], target: EditTarget) -> String {
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
    fn apply_edit_in(
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

    /// Insert an empty row at `index` into a plain headers/rows model.
    ///
    /// ### Description
    /// The index is clamped to the current row count, so an out-of-bounds index
    /// appends. The inserted row is padded to the header width.
    ///
    /// ### Arguments
    /// - `headers`: The column headers, used to size the new row
    /// - `rows`: The data rows to mutate
    /// - `index`: The row index to insert at
    fn insert_row_at_in(headers: &[String], rows: &mut Vec<Vec<String>>, index: usize) {
        let index = index.min(rows.len());
        rows.insert(index, vec![String::new(); headers.len()]);
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
        Self::insert_row_above_in(&self.headers, &mut self.rows, selected);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty row above the selected row into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers, used to size the new row
    /// - `rows`: The data rows to mutate
    /// - `selected`: The currently selected `(row, grid column)`, if any; when
    ///   `None` the row is appended at the end
    fn insert_row_above_in(
        headers: &[String],
        rows: &mut Vec<Vec<String>>,
        selected: Option<(usize, usize)>,
    ) {
        let index = selected.map_or(rows.len(), |(row, _)| row);
        Self::insert_row_at_in(headers, rows, index);
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
        Self::insert_row_below_in(&self.headers, &mut self.rows, selected);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty row below the selected row into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers, used to size the new row
    /// - `rows`: The data rows to mutate
    /// - `selected`: The currently selected `(row, grid column)`, if any; when
    ///   `None` the row is appended at the end
    fn insert_row_below_in(
        headers: &[String],
        rows: &mut Vec<Vec<String>>,
        selected: Option<(usize, usize)>,
    ) {
        let index = selected.map_or(rows.len(), |(row, _)| row + 1);
        Self::insert_row_at_in(headers, rows, index);
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
        if Self::delete_row_in(&mut self.rows, selected) {
            self.commit_and_refresh(window, cx);
        }
    }

    /// Delete the selected row from a plain model, clamping out-of-bounds
    /// selections to the last row.
    ///
    /// ### Arguments
    /// - `rows`: The data rows to mutate
    /// - `selected`: The currently selected `(row, grid column)`, if any; when
    ///   `None` the last row is removed
    ///
    /// ### Returns
    /// - `true`: A row was removed
    /// - `false`: The buffer was empty and nothing changed
    fn delete_row_in(rows: &mut Vec<Vec<String>>, selected: Option<(usize, usize)>) -> bool {
        if rows.is_empty() {
            return false;
        }
        let last = rows.len() - 1;
        let index = selected.map_or(last, |(row, _)| row).min(last);
        rows.remove(index);
        true
    }

    /// Insert an empty data column at `index` into a plain headers/rows model.
    ///
    /// ### Description
    /// The header index is clamped to the header count, and each row's insertion
    /// position is independently clamped to that row's width, so ragged rows stay
    /// in bounds.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `index`: The column index to insert at
    fn insert_column_at_in(headers: &mut Vec<String>, rows: &mut [Vec<String>], index: usize) {
        let index = index.min(headers.len());
        headers.insert(index, String::new());
        for row in rows {
            let position = index.min(row.len());
            row.insert(position, String::new());
        }
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
        Self::insert_column_before_in(&mut self.headers, &mut self.rows, selected);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column before the selected column into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `selected`: The currently selected `(row, grid column)`, if any; when
    ///   `None` the column is appended at the end
    fn insert_column_before_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        selected: Option<(usize, usize)>,
    ) {
        let index = Self::selected_data_column(selected).unwrap_or(headers.len());
        Self::insert_column_at_in(headers, rows, index);
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
        Self::insert_column_after_in(&mut self.headers, &mut self.rows, selected);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column after the selected column into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `selected`: The currently selected `(row, grid column)`, if any; when
    ///   `None` the column is appended at the end
    fn insert_column_after_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        selected: Option<(usize, usize)>,
    ) {
        let index = Self::selected_data_column(selected).map_or(headers.len(), |col| col + 1);
        Self::insert_column_at_in(headers, rows, index);
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
        if Self::delete_column_in(&mut self.headers, &mut self.rows, selected) {
            self.commit_and_refresh(window, cx);
        }
    }

    /// Delete the selected column from a plain model, clamping out-of-bounds
    /// selections to the last column.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `selected`: The currently selected `(row, grid column)`, if any; when
    ///   `None` the last column is removed
    ///
    /// ### Returns
    /// - `true`: A column was removed
    /// - `false`: There were no columns and nothing changed
    fn delete_column_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        selected: Option<(usize, usize)>,
    ) -> bool {
        if headers.is_empty() {
            return false;
        }
        let last = headers.len() - 1;
        let index = Self::selected_data_column(selected)
            .unwrap_or(last)
            .min(last);
        headers.remove(index);
        for row in rows {
            if index < row.len() {
                row.remove(index);
            }
        }
        true
    }

    /// Move a grid column to a new position within a plain headers/rows model.
    ///
    /// ### Description
    /// Grid indices include the synthetic row-number column at index 0, which is
    /// not movable. The source is rejected when it is the row-number column or
    /// out of bounds, and the destination is clamped to the last data column.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `col_ix`: The source grid column index
    /// - `to_ix`: The destination grid column index
    ///
    /// ### Returns
    /// - `true`: A column was moved
    /// - `false`: The move was a no-op (row-number column, out of bounds, or same
    ///   position)
    fn move_column_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        col_ix: usize,
        to_ix: usize,
    ) -> bool {
        // The row-number column (grid index 0) is not movable.
        let Some(from_data) = col_ix.checked_sub(1) else {
            return false;
        };
        if from_data >= headers.len() {
            return false;
        }
        let to_data = to_ix.saturating_sub(1).min(headers.len() - 1);
        if from_data == to_data {
            return false;
        }

        let header = headers.remove(from_data);
        headers.insert(to_data, header);
        for row in rows {
            if from_data < row.len() {
                let cell = row.remove(from_data);
                row.insert(to_data.min(row.len()), cell);
            }
        }
        true
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
        if Self::move_column_in(&mut self.headers, &mut self.rows, col_ix, to_ix) {
            self.commit_and_refresh(window, cx);
        }
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

#[cfg(test)]
mod tests {
    use super::{CsvTableDelegate, EditTarget};
    use crate::fulgur::files::csv_support::serialize_csv;

    /// Build a `Vec<String>` from string literals for terser test fixtures.
    fn row(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    /// A small two-column, two-row fixture shared across tests.
    fn sample() -> (Vec<String>, Vec<Vec<String>>) {
        (row(&["a", "b"]), vec![row(&["1", "2"]), row(&["3", "4"])])
    }

    #[test]
    fn test_compute_columns_has_row_number_plus_one_per_header() {
        let (headers, rows) = sample();
        let columns = CsvTableDelegate::compute_columns(&headers, &rows);
        assert_eq!(columns.len(), headers.len() + 1);
    }

    #[test]
    fn test_selected_data_column_ignores_row_number_column() {
        assert_eq!(CsvTableDelegate::selected_data_column(None), None);
        assert_eq!(CsvTableDelegate::selected_data_column(Some((0, 0))), None);
        assert_eq!(
            CsvTableDelegate::selected_data_column(Some((0, 1))),
            Some(0)
        );
        assert_eq!(
            CsvTableDelegate::selected_data_column(Some((3, 2))),
            Some(1)
        );
    }

    #[test]
    fn test_value_at_in_reads_header_and_cell() {
        let (headers, rows) = sample();
        assert_eq!(
            CsvTableDelegate::value_at_in(&headers, &rows, EditTarget::Header(1)),
            "b"
        );
        assert_eq!(
            CsvTableDelegate::value_at_in(&headers, &rows, EditTarget::Cell(1, 0)),
            "3"
        );
    }

    #[test]
    fn test_value_at_in_out_of_bounds_is_empty() {
        let (headers, rows) = sample();
        assert_eq!(
            CsvTableDelegate::value_at_in(&headers, &rows, EditTarget::Header(9)),
            ""
        );
        assert_eq!(
            CsvTableDelegate::value_at_in(&headers, &rows, EditTarget::Cell(9, 9)),
            ""
        );
    }

    #[test]
    fn test_apply_edit_in_sets_header_and_cell() {
        let (mut headers, mut rows) = sample();
        CsvTableDelegate::apply_edit_in(
            &mut headers,
            &mut rows,
            EditTarget::Header(0),
            "name".to_string(),
        );
        CsvTableDelegate::apply_edit_in(
            &mut headers,
            &mut rows,
            EditTarget::Cell(0, 1),
            "x".to_string(),
        );
        assert_eq!(headers, row(&["name", "b"]));
        assert_eq!(rows[0], row(&["1", "x"]));
    }

    #[test]
    fn test_apply_edit_in_out_of_bounds_is_noop() {
        let (mut headers, mut rows) = sample();
        CsvTableDelegate::apply_edit_in(
            &mut headers,
            &mut rows,
            EditTarget::Cell(9, 9),
            "x".to_string(),
        );
        assert_eq!(headers, row(&["a", "b"]));
        assert_eq!(rows, vec![row(&["1", "2"]), row(&["3", "4"])]);
    }

    #[test]
    fn test_insert_row_at_in_start_middle_end_and_out_of_bounds() {
        let (headers, _) = sample();

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        CsvTableDelegate::insert_row_at_in(&headers, &mut rows, 0);
        assert_eq!(rows[0], row(&["", ""]));

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        CsvTableDelegate::insert_row_at_in(&headers, &mut rows, 1);
        assert_eq!(rows[1], row(&["", ""]));
        assert_eq!(rows.len(), 3);

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        CsvTableDelegate::insert_row_at_in(&headers, &mut rows, 2);
        assert_eq!(rows[2], row(&["", ""]));

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        CsvTableDelegate::insert_row_at_in(&headers, &mut rows, 99);
        assert_eq!(rows.len(), 3);
        assert_eq!(rows[2], row(&["", ""]));
    }

    #[test]
    fn test_insert_row_above_and_below_resolve_selection() {
        let (headers, _) = sample();

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        CsvTableDelegate::insert_row_above_in(&headers, &mut rows, Some((1, 1)));
        assert_eq!(rows[1], row(&["", ""]));
        assert_eq!(rows[2], row(&["3", "4"]));

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        CsvTableDelegate::insert_row_below_in(&headers, &mut rows, Some((0, 1)));
        assert_eq!(rows[1], row(&["", ""]));
        assert_eq!(rows[0], row(&["1", "2"]));

        let mut rows = vec![row(&["1", "2"])];
        CsvTableDelegate::insert_row_above_in(&headers, &mut rows, None);
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[1], row(&["", ""]));
    }

    #[test]
    fn test_delete_row_in_selected_last_and_empty() {
        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        assert!(CsvTableDelegate::delete_row_in(&mut rows, Some((0, 1))));
        assert_eq!(rows, vec![row(&["3", "4"])]);

        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        assert!(CsvTableDelegate::delete_row_in(&mut rows, None));
        assert_eq!(rows, vec![row(&["1", "2"])]);

        // Out-of-bounds selection clamps to the last row.
        let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
        assert!(CsvTableDelegate::delete_row_in(&mut rows, Some((9, 1))));
        assert_eq!(rows, vec![row(&["1", "2"])]);

        let mut rows: Vec<Vec<String>> = Vec::new();
        assert!(!CsvTableDelegate::delete_row_in(&mut rows, None));
    }

    #[test]
    fn test_insert_column_at_in_shifts_cells_and_pads_ragged_rows() {
        let mut headers = row(&["a", "b"]);
        let mut rows = vec![row(&["1", "2"]), row(&["3"])];
        CsvTableDelegate::insert_column_at_in(&mut headers, &mut rows, 1);
        assert_eq!(headers, row(&["a", "", "b"]));
        assert_eq!(rows[0], row(&["1", "", "2"]));
        // The ragged row gets the new cell clamped to its own width.
        assert_eq!(rows[1], row(&["3", ""]));
    }

    #[test]
    fn test_insert_column_before_and_after_resolve_selection() {
        let mut headers = row(&["a", "b"]);
        let mut rows = vec![row(&["1", "2"])];
        CsvTableDelegate::insert_column_before_in(&mut headers, &mut rows, Some((0, 2)));
        assert_eq!(headers, row(&["a", "", "b"]));
        assert_eq!(rows[0], row(&["1", "", "2"]));

        let mut headers = row(&["a", "b"]);
        let mut rows = vec![row(&["1", "2"])];
        CsvTableDelegate::insert_column_after_in(&mut headers, &mut rows, Some((0, 1)));
        assert_eq!(headers, row(&["a", "", "b"]));
        assert_eq!(rows[0], row(&["1", "", "2"]));

        let mut headers = row(&["a", "b"]);
        let mut rows = vec![row(&["1", "2"])];
        CsvTableDelegate::insert_column_before_in(&mut headers, &mut rows, None);
        assert_eq!(headers, row(&["a", "b", ""]));
        assert_eq!(rows[0], row(&["1", "2", ""]));
    }

    #[test]
    fn test_delete_column_in_selected_last_and_empty() {
        let mut headers = row(&["a", "b", "c"]);
        let mut rows = vec![row(&["1", "2", "3"])];
        assert!(CsvTableDelegate::delete_column_in(
            &mut headers,
            &mut rows,
            Some((0, 2))
        ));
        assert_eq!(headers, row(&["a", "c"]));
        assert_eq!(rows[0], row(&["1", "3"]));

        // None deletes the last column.
        let mut headers = row(&["a", "b", "c"]);
        let mut rows = vec![row(&["1", "2", "3"])];
        assert!(CsvTableDelegate::delete_column_in(
            &mut headers,
            &mut rows,
            None
        ));
        assert_eq!(headers, row(&["a", "b"]));
        assert_eq!(rows[0], row(&["1", "2"]));

        let mut headers: Vec<String> = Vec::new();
        let mut rows: Vec<Vec<String>> = Vec::new();
        assert!(!CsvTableDelegate::delete_column_in(
            &mut headers,
            &mut rows,
            None
        ));
    }

    #[test]
    fn test_move_column_in_reorders_header_and_cells() {
        let mut headers = row(&["a", "b", "c"]);
        let mut rows = vec![row(&["1", "2", "3"])];
        // Grid index 1 is data column 0; move it after data column 2.
        assert!(CsvTableDelegate::move_column_in(
            &mut headers,
            &mut rows,
            1,
            3
        ));
        assert_eq!(headers, row(&["b", "c", "a"]));
        assert_eq!(rows[0], row(&["2", "3", "1"]));
    }

    #[test]
    fn test_move_column_in_rejects_row_number_column_and_noops() {
        let (mut headers, mut rows) = sample();

        // The row-number column (grid index 0) cannot be moved.
        assert!(!CsvTableDelegate::move_column_in(
            &mut headers,
            &mut rows,
            0,
            2
        ));
        // Out-of-bounds source.
        assert!(!CsvTableDelegate::move_column_in(
            &mut headers,
            &mut rows,
            9,
            1
        ));
        // Same source and destination.
        assert!(!CsvTableDelegate::move_column_in(
            &mut headers,
            &mut rows,
            1,
            1
        ));
        assert_eq!(headers, row(&["a", "b"]));
        assert_eq!(rows, vec![row(&["1", "2"]), row(&["3", "4"])]);
    }

    #[test]
    fn test_edits_round_trip_through_serialize_csv() {
        let (mut headers, mut rows) = sample();
        CsvTableDelegate::insert_row_below_in(&headers, &mut rows, Some((0, 1)));
        CsvTableDelegate::apply_edit_in(
            &mut headers,
            &mut rows,
            EditTarget::Cell(1, 0),
            "5".to_string(),
        );
        assert_eq!(
            serialize_csv(&headers, &rows, b','),
            Ok("a,b\n1,2\n5,\n3,4\n".to_string())
        );
    }
}
