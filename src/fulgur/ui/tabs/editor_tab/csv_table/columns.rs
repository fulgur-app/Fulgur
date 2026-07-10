//! Column insert, delete, and move operations.

use gpui::{Context, Window};
use gpui_component::table::TableState;

use super::CsvTableDelegate;

impl CsvTableDelegate {
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
    pub(super) fn insert_column_at_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        index: usize,
    ) {
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
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_column_before(
        &mut self,
        window: &mut Window,
        cx: &mut Context<TableState<Self>>,
    ) {
        let selected_col = self.selected_data_column_index();
        Self::insert_column_before_in(&mut self.headers, &mut self.rows, selected_col);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column before the selected column into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `selected_col`: The currently selected data column, if any; when
    ///   `None` the column is appended at the end
    pub(super) fn insert_column_before_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        selected_col: Option<usize>,
    ) {
        let index = selected_col.unwrap_or(headers.len());
        Self::insert_column_at_in(headers, rows, index);
    }

    /// Insert an empty column after the selected column (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_column_after(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_col = self.selected_data_column_index();
        Self::insert_column_after_in(&mut self.headers, &mut self.rows, selected_col);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty column after the selected column into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `selected_col`: The currently selected data column, if any; when
    ///   `None` the column is appended at the end
    pub(super) fn insert_column_after_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        selected_col: Option<usize>,
    ) {
        let index = selected_col.map_or(headers.len(), |col| col + 1);
        Self::insert_column_at_in(headers, rows, index);
    }

    /// Delete the selected column, or the last column if none is selected.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn delete_column(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_col = self.selected_data_column_index();
        if Self::delete_column_in(&mut self.headers, &mut self.rows, selected_col) {
            self.commit_and_refresh(window, cx);
        }
    }

    /// Delete the selected column from a plain model, clamping out-of-bounds
    /// selections to the last column.
    ///
    /// ### Arguments
    /// - `headers`: The column headers to mutate
    /// - `rows`: The data rows to mutate
    /// - `selected_col`: The currently selected data column, if any; when
    ///   `None` the last column is removed
    ///
    /// ### Returns
    /// - `true`: A column was removed
    /// - `false`: There were no columns and nothing changed
    pub(super) fn delete_column_in(
        headers: &mut Vec<String>,
        rows: &mut [Vec<String>],
        selected_col: Option<usize>,
    ) -> bool {
        if headers.is_empty() {
            return false;
        }
        let last = headers.len() - 1;
        let index = selected_col.unwrap_or(last).min(last);
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
    pub(super) fn move_column_in(
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
