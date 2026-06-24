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
