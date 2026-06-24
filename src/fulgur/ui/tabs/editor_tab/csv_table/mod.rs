//! The table delegate backing the CSV grid view.

mod columns;
mod delegate;
mod edits;
mod rows;
#[cfg(test)]
mod tests;

use gpui::{Context, Entity, SharedString, Window, px};
use gpui_component::{
    input::{InputEvent, InputState},
    table::{Column, TableState},
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
}
