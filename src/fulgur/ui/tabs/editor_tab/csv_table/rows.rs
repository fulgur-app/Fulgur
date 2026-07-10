//! Row insert and delete operations.

use gpui::{Context, Window};
use gpui_component::table::TableState;

use super::CsvTableDelegate;

impl CsvTableDelegate {
    /// Insert an empty row at `index` into a plain rows model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers, used to size the new row
    /// - `rows`: The data rows to mutate
    /// - `index`: The row index to insert at, clamped to the row count
    pub(super) fn insert_row_at_in(headers: &[String], rows: &mut Vec<Vec<String>>, index: usize) {
        let index = index.min(rows.len());
        rows.insert(index, vec![String::new(); headers.len()]);
    }

    /// Insert an empty row above the selected row (or at the end if none).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_row_above(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_row = self.selected_row_index();
        Self::insert_row_above_in(&self.headers, &mut self.rows, selected_row);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty row above the selected row into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers, used to size the new row
    /// - `rows`: The data rows to mutate
    /// - `selected_row`: The currently selected row, if any; when `None` the
    ///   row is appended at the end
    pub(super) fn insert_row_above_in(
        headers: &[String],
        rows: &mut Vec<Vec<String>>,
        selected_row: Option<usize>,
    ) {
        let index = selected_row.unwrap_or(rows.len());
        Self::insert_row_at_in(headers, rows, index);
    }

    /// Insert an empty row below the selected row (or at the end if none).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn insert_row_below(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_row = self.selected_row_index();
        Self::insert_row_below_in(&self.headers, &mut self.rows, selected_row);
        self.commit_and_refresh(window, cx);
    }

    /// Insert an empty row below the selected row into a plain model.
    ///
    /// ### Arguments
    /// - `headers`: The column headers, used to size the new row
    /// - `rows`: The data rows to mutate
    /// - `selected_row`: The currently selected row, if any; when `None` the
    ///   row is appended at the end
    pub(super) fn insert_row_below_in(
        headers: &[String],
        rows: &mut Vec<Vec<String>>,
        selected_row: Option<usize>,
    ) {
        let index = selected_row.map_or(rows.len(), |row| row + 1);
        Self::insert_row_at_in(headers, rows, index);
    }

    /// Delete the selected row, or the last row if none is selected.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The table state context
    pub fn delete_row(&mut self, window: &mut Window, cx: &mut Context<TableState<Self>>) {
        let selected_row = self.selected_row_index();
        if Self::delete_row_in(&mut self.rows, selected_row) {
            self.commit_and_refresh(window, cx);
        }
    }

    /// Delete the selected row from a plain model, clamping out-of-bounds
    /// selections to the last row.
    ///
    /// ### Arguments
    /// - `rows`: The data rows to mutate
    /// - `selected_row`: The currently selected row, if any; when `None` the
    ///   last row is removed
    ///
    /// ### Returns
    /// - `true`: A row was removed
    /// - `false`: The buffer was empty and nothing changed
    pub(super) fn delete_row_in(rows: &mut Vec<Vec<String>>, selected_row: Option<usize>) -> bool {
        if rows.is_empty() {
            return false;
        }
        let last = rows.len() - 1;
        let index = selected_row.unwrap_or(last).min(last);
        rows.remove(index);
        true
    }
}
