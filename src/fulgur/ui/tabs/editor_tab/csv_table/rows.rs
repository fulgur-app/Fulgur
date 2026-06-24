//! Row insert and delete operations.

use gpui::{Context, Window};
use gpui_component::table::TableState;

use super::CsvTableDelegate;

impl CsvTableDelegate {
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
    pub(super) fn insert_row_at_in(headers: &[String], rows: &mut Vec<Vec<String>>, index: usize) {
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
    pub(super) fn insert_row_above_in(
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
    pub(super) fn insert_row_below_in(
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
    pub(super) fn delete_row_in(
        rows: &mut Vec<Vec<String>>,
        selected: Option<(usize, usize)>,
    ) -> bool {
        if rows.is_empty() {
            return false;
        }
        let last = rows.len() - 1;
        let index = selected.map_or(last, |(row, _)| row).min(last);
        rows.remove(index);
        true
    }
}
