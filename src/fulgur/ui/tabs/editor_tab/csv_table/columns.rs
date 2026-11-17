//! Column insert, delete, and move operations.

use gpui_kit::component::table::TableState;
use gpui_kit::{Context, Window};

use super::CsvTableDelegate;
use crate::fulgur::files::csv_support::header_record;

/// Whether a record of `len` fields receives a column inserted at `index`.
///
/// ### Arguments
/// - `len`: The record's field count
/// - `index`: The clamped data column index of the insertion
/// - `width`: The header count before the insertion
///
/// ### Returns
/// - `true`: The record gains a field
/// - `false`: The insertion falls in the record's missing fields
const fn record_receives_inserted_column(len: usize, index: usize, width: usize) -> bool {
    index < len || len >= width
}

/// The field count a record keeps after one of its cells moves, before any
/// non-empty cell beyond it is accounted for.
///
/// ### Arguments
/// - `len`: The record's field count before the move
/// - `from`: The source data column index
/// - `to`: The destination data column index
///
/// ### Returns
/// - `usize`: The record's field count after the move
const fn record_len_after_move(len: usize, from: usize, to: usize) -> usize {
    if from >= len && to >= len {
        return len;
    }
    let mut kept_len = len;
    if from < kept_len {
        kept_len -= 1;
    }
    if to <= kept_len {
        kept_len += 1;
    }
    kept_len
}

impl CsvTableDelegate {
    /// Insert an empty data column at `index` into a plain headers/rows model.
    ///
    /// ### Description
    /// The header index is clamped to the header count. A row, or a header
    /// record, shorter than the header width only receives the new cell when
    /// it lands among its existing fields, so its missing trailing fields stay
    /// missing in the file.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `header_len`: The header record's field count to keep in step
    /// - `rows`: The data rows to mutate
    /// - `index`: The column index to insert at
    pub(super) fn insert_column_at_in(
        headers: &mut Vec<String>,
        header_len: &mut usize,
        rows: &mut [Vec<String>],
        index: usize,
    ) {
        let width = headers.len();
        let index = index.min(width);
        let record_len = header_record(headers, *header_len).len();
        *header_len =
            record_len + usize::from(record_receives_inserted_column(record_len, index, width));
        headers.insert(index, String::new());
        for row in rows {
            if record_receives_inserted_column(row.len(), index, width) {
                row.insert(index.min(row.len()), String::new());
            }
        }
    }

    /// Insert an empty column before the selected column (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_column_before(
        &mut self,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let selected_col = self.selected_data_column_index();
        Self::insert_column_before_in(
            &mut self.headers,
            &mut self.header_len,
            &mut self.rows,
            selected_col,
        );
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column before the selected column into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `header_len`: The header record's field count to keep in step
    /// - `rows`: The data rows to mutate
    /// - `selected_col`: The currently selected data column, if any; when
    ///   `None` the column is appended at the end
    pub(super) fn insert_column_before_in(
        headers: &mut Vec<String>,
        header_len: &mut usize,
        rows: &mut [Vec<String>],
        selected_col: Option<usize>,
    ) {
        let index = selected_col.unwrap_or(headers.len());
        Self::insert_column_at_in(headers, header_len, rows, index);
    }

    /// Insert an empty column after the selected column (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_column_after(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_col = self.selected_data_column_index();
        Self::insert_column_after_in(
            &mut self.headers,
            &mut self.header_len,
            &mut self.rows,
            selected_col,
        );
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column after the selected column into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `header_len`: The header record's field count to keep in step
    /// - `rows`: The data rows to mutate
    /// - `selected_col`: The currently selected data column, if any; when
    ///   `None` the column is appended at the end
    pub(super) fn insert_column_after_in(
        headers: &mut Vec<String>,
        header_len: &mut usize,
        rows: &mut [Vec<String>],
        selected_col: Option<usize>,
    ) {
        let index = selected_col.map_or(headers.len(), |col| col + 1);
        Self::insert_column_at_in(headers, header_len, rows, index);
    }

    /// Delete the selected column, or the last column if none is selected.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn delete_column(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_col = self.selected_data_column_index();
        if Self::delete_column_in(
            &mut self.headers,
            &mut self.header_len,
            &mut self.rows,
            selected_col,
        ) {
            self.commit_and_refresh(window, cx);
        }
    }

    /// Delete the selected column from a plain model, clamping out-of-bounds
    /// selections to the last column.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `header_len`: The header record's field count to keep in step
    /// - `rows`: The data rows to mutate
    /// - `selected_col`: The currently selected data column, if any; when
    ///   `None` the last column is removed
    ///
    /// ### Returns
    /// - `true`: A column was removed
    /// - `false`: There were no columns and nothing changed
    pub(super) fn delete_column_in(
        headers: &mut Vec<String>,
        header_len: &mut usize,
        rows: &mut [Vec<String>],
        selected_col: Option<usize>,
    ) -> bool {
        if headers.is_empty() {
            return false;
        }
        let last = headers.len() - 1;
        let index = selected_col.unwrap_or(last).min(last);
        let record_len = header_record(headers, *header_len).len();
        *header_len = record_len - usize::from(index < record_len);
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
    /// - `header_len`: The header record's field count to keep in step
    /// - `rows`: The data rows to mutate
    /// - `col_ix`: The source grid column index
    /// - `to_ix`: The destination grid column index
    ///
    /// ### Returns
    /// - `true`: A column was moved
    /// - `false`: The move was a no-op (row-number column, out of bounds, or same
    ///   position)
    pub(super) fn move_column_in(
        headers: &mut Vec<String>,
        header_len: &mut usize,
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

        let width = headers.len();
        let record_len = header_record(headers, *header_len).len();
        *header_len = record_len_after_move(record_len, from_data, to_data);
        let header = headers.remove(from_data);
        headers.insert(to_data, header);
        for row in rows {
            Self::move_cell_in_row(row, from_data, to_data, width);
        }
        true
    }

    /// Move one cell of a possibly short row, as if the row were padded to the
    /// header width, then drop the padding that is still empty.
    ///
    /// ### Arguments
    /// - `row`: The data row to mutate
    /// - `from`: The source data column index, below `width`
    /// - `to`: The destination data column index, below `width`
    /// - `width`: The header count the row is displayed against
    fn move_cell_in_row(row: &mut Vec<String>, from: usize, to: usize, width: usize) {
        let original_len = row.len();
        if from >= original_len && to >= original_len {
            return;
        }
        row.resize(width.max(original_len), String::new());
        let cell = row.remove(from);
        row.insert(to, cell);

        let kept_len = record_len_after_move(original_len, from, to);
        while row.len() > kept_len && row.last().is_some_and(String::is_empty) {
            row.pop();
        }
    }
}
