//! Fit-to-content measurement for the data columns of the CSV grid.

use std::ops::Range;

use gpui_kit::component::ActiveTheme;
use gpui_kit::{App, Pixels, TextRun, Window, font, px, rems};

use super::CsvTableDelegate;

/// Number of evenly spaced rows measured across a large column, on top of the
/// visible rows and the column's longest cell.
const FIT_SAMPLE_ROWS: usize = 500;
/// Characters of a line that get measured. Enough to reach the table's maximum
/// column width with narrow glyphs, while a huge cell stays cheap to shape.
const FIT_MAX_LINE_CHARS: usize = 1024;
/// Text size of `DataTable` cells at its default size, which applies `text_sm`.
const TABLE_TEXT_SIZE_REMS: f32 = 0.875;

/// Select the rows whose cells are measured to fit a data column.
///
/// ### Arguments
/// - `rows`: The data rows
/// - `data_col`: The data column index
/// - `visible_rows`: The row range the table last reported as visible
///
/// ### Returns
/// - `Vec<usize>`: The indices of the rows to measure, possibly with duplicates
fn rows_to_measure(
    rows: &[Vec<String>],
    data_col: usize,
    visible_rows: &Range<usize>,
) -> Vec<usize> {
    if rows.len() <= FIT_SAMPLE_ROWS {
        return (0..rows.len()).collect();
    }
    let visible = visible_rows.start.min(rows.len())..visible_rows.end.min(rows.len());
    let longest = (0..rows.len()).max_by_key(|&row_ix| {
        rows[row_ix]
            .get(data_col)
            .map_or(0, std::string::String::len)
    });
    (0..rows.len())
        .step_by(rows.len().div_ceil(FIT_SAMPLE_ROWS))
        .chain(visible)
        .chain(longest)
        .collect()
}

/// Cut a line down to the part of it that gets measured.
///
/// ### Arguments
/// - `line`: The line of text
///
/// ### Returns
/// - `&str`: At most `FIT_MAX_LINE_CHARS` leading characters of the line
fn measurable_prefix(line: &str) -> &str {
    line.char_indices()
        .nth(FIT_MAX_LINE_CHARS)
        .map_or(line, |(byte_ix, _)| &line[..byte_ix])
}

impl CsvTableDelegate {
    /// Measure the width a data column needs to show its header and cells whole.
    ///
    /// ### Arguments
    /// - `col_ix`: The grid column index, where 0 is the row-number column
    /// - `window`: The window whose text system measures the text
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Pixels)`: The content width, without the cell padding
    /// - `None`: The column is the row-number column, or is out of bounds
    pub(super) fn measure_column_content(
        &self,
        col_ix: usize,
        window: &Window,
        cx: &App,
    ) -> Option<Pixels> {
        let data_col = col_ix.checked_sub(1)?;
        let header = self.headers.get(data_col)?;

        let font_size = rems(TABLE_TEXT_SIZE_REMS).to_pixels(window.rem_size());
        let table_font = font(cx.theme().font_family.clone());
        let color = cx.theme().foreground;
        let text_width = |text: &str| {
            text.lines()
                .filter(|line| !line.is_empty())
                .map(|line| {
                    let line = measurable_prefix(line);
                    let run = TextRun {
                        len: line.len(),
                        font: table_font.clone(),
                        color,
                        background_color: None,
                        underline: None,
                        strikethrough: None,
                    };
                    window
                        .text_system()
                        .layout_line(line, font_size, &[run], None)
                        .width
                })
                .fold(px(0.), Pixels::max)
        };

        let widest_cell = rows_to_measure(&self.rows, data_col, &self.visible_rows)
            .into_iter()
            .filter_map(|row_ix| self.rows[row_ix].get(data_col))
            .map(|cell| text_width(cell))
            .fold(px(0.), Pixels::max);
        Some(widest_cell.max(text_width(header)))
    }
}

#[cfg(test)]
mod tests {
    use super::{FIT_MAX_LINE_CHARS, FIT_SAMPLE_ROWS, measurable_prefix, rows_to_measure};

    /// Build a one-column table of `count` short cells.
    fn short_rows(count: usize) -> Vec<Vec<String>> {
        vec![vec!["1".to_string()]; count]
    }

    #[test]
    fn test_rows_to_measure_takes_every_row_of_a_small_table() {
        let rows = short_rows(FIT_SAMPLE_ROWS);
        let expected: Vec<usize> = (0..FIT_SAMPLE_ROWS).collect();
        assert_eq!(rows_to_measure(&rows, 0, &(0..0)), expected);
    }

    #[test]
    fn test_rows_to_measure_bounds_a_large_table_and_keeps_visible_and_longest() {
        let mut rows = short_rows(FIT_SAMPLE_ROWS * 20);
        rows[1237][0] = "the longest cell".to_string();
        let visible = 11..13;

        let measured = rows_to_measure(&rows, 0, &visible);

        assert!(measured.len() <= FIT_SAMPLE_ROWS + visible.len() + 1);
        assert!(measured.contains(&1237));
        assert!(measured.contains(&11));
        assert!(measured.contains(&12));
    }

    #[test]
    fn test_rows_to_measure_clamps_a_stale_visible_range() {
        let rows = short_rows(FIT_SAMPLE_ROWS + 1);
        let measured = rows_to_measure(&rows, 0, &(FIT_SAMPLE_ROWS..FIT_SAMPLE_ROWS * 2));
        assert!(measured.iter().all(|&row_ix| row_ix < rows.len()));
    }

    #[test]
    fn test_rows_to_measure_tolerates_rows_shorter_than_the_column() {
        let rows = short_rows(FIT_SAMPLE_ROWS + 1);
        assert!(rows_to_measure(&rows, 3, &(0..0)).contains(&0));
    }

    #[test]
    fn test_measurable_prefix_keeps_a_short_line_whole() {
        assert_eq!(measurable_prefix("short"), "short");
    }

    #[test]
    fn test_measurable_prefix_cuts_on_a_character_boundary() {
        let line = "\u{1F600}".repeat(FIT_MAX_LINE_CHARS + 5);
        let prefix = measurable_prefix(&line);
        assert_eq!(prefix.chars().count(), FIT_MAX_LINE_CHARS);
    }
}
