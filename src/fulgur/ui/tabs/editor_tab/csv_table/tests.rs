use super::{CsvSelection, CsvTableDelegate, EditTarget};
use crate::fulgur::files::csv_support::{CsvDialect, serialize_csv};

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
fn test_selected_data_column_of_ignores_row_number_column_and_rows() {
    assert_eq!(CsvTableDelegate::selected_data_column_of(None), None);
    assert_eq!(
        CsvTableDelegate::selected_data_column_of(Some(CsvSelection::Cell(0, 0))),
        None
    );
    assert_eq!(
        CsvTableDelegate::selected_data_column_of(Some(CsvSelection::Cell(0, 1))),
        Some(0)
    );
    assert_eq!(
        CsvTableDelegate::selected_data_column_of(Some(CsvSelection::Cell(3, 2))),
        Some(1)
    );
    assert_eq!(
        CsvTableDelegate::selected_data_column_of(Some(CsvSelection::Column(0))),
        None
    );
    assert_eq!(
        CsvTableDelegate::selected_data_column_of(Some(CsvSelection::Column(2))),
        Some(1)
    );
    assert_eq!(
        CsvTableDelegate::selected_data_column_of(Some(CsvSelection::Row(3))),
        None
    );
}

#[test]
fn test_selected_row_of_ignores_column_selections() {
    assert_eq!(CsvTableDelegate::selected_row_of(None), None);
    assert_eq!(
        CsvTableDelegate::selected_row_of(Some(CsvSelection::Cell(3, 1))),
        Some(3)
    );
    assert_eq!(
        CsvTableDelegate::selected_row_of(Some(CsvSelection::Row(2))),
        Some(2)
    );
    assert_eq!(
        CsvTableDelegate::selected_row_of(Some(CsvSelection::Column(1))),
        None
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
    CsvTableDelegate::insert_row_above_in(&headers, &mut rows, Some(1));
    assert_eq!(rows[1], row(&["", ""]));
    assert_eq!(rows[2], row(&["3", "4"]));

    let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
    CsvTableDelegate::insert_row_below_in(&headers, &mut rows, Some(0));
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
    assert!(CsvTableDelegate::delete_row_in(&mut rows, Some(0)));
    assert_eq!(rows, vec![row(&["3", "4"])]);

    let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
    assert!(CsvTableDelegate::delete_row_in(&mut rows, None));
    assert_eq!(rows, vec![row(&["1", "2"])]);

    // Out-of-bounds selection clamps to the last row.
    let mut rows = vec![row(&["1", "2"]), row(&["3", "4"])];
    assert!(CsvTableDelegate::delete_row_in(&mut rows, Some(9)));
    assert_eq!(rows, vec![row(&["1", "2"])]);

    let mut rows: Vec<Vec<String>> = Vec::new();
    assert!(!CsvTableDelegate::delete_row_in(&mut rows, None));
}

#[test]
fn test_insert_column_at_in_leaves_short_rows_short() {
    let mut headers = row(&["a", "b", "c"]);
    let mut rows = vec![row(&["1", "2", "3"]), row(&["4", "5"]), row(&["6"])];
    CsvTableDelegate::insert_column_at_in(&mut headers, &mut rows, 1);
    assert_eq!(headers, row(&["a", "", "b", "c"]));
    assert_eq!(rows[0], row(&["1", "", "2", "3"]));
    assert_eq!(rows[1], row(&["4", "", "5"]));
    // The new column falls in the short row's missing fields.
    assert_eq!(rows[2], row(&["6"]));

    // Appending a column grows full rows only.
    CsvTableDelegate::insert_column_at_in(&mut headers, &mut rows, 4);
    assert_eq!(rows[0], row(&["1", "", "2", "3", ""]));
    assert_eq!(rows[1], row(&["4", "", "5"]));
}

#[test]
fn test_insert_column_before_and_after_resolve_selection() {
    let mut headers = row(&["a", "b"]);
    let mut rows = vec![row(&["1", "2"])];
    CsvTableDelegate::insert_column_before_in(&mut headers, &mut rows, Some(1));
    assert_eq!(headers, row(&["a", "", "b"]));
    assert_eq!(rows[0], row(&["1", "", "2"]));

    let mut headers = row(&["a", "b"]);
    let mut rows = vec![row(&["1", "2"])];
    CsvTableDelegate::insert_column_after_in(&mut headers, &mut rows, Some(0));
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
        Some(1)
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
fn test_move_column_in_keeps_short_rows_aligned() {
    let mut headers = row(&["a", "b", "c"]);
    let mut rows = vec![row(&["1"]), row(&["2", "3"]), row(&["4", "", "5"])];
    // Move data column 2 to the front.
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut rows,
        3,
        1
    ));
    assert_eq!(headers, row(&["c", "a", "b"]));
    assert_eq!(rows[0], row(&["", "1"]));
    assert_eq!(rows[1], row(&["", "2", "3"]));
    assert_eq!(rows[2], row(&["5", "4", ""]));

    // Moving a value past a short row's end extends it.
    let mut headers = row(&["a", "b", "c"]);
    let mut rows = vec![row(&["1"])];
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut rows,
        1,
        3
    ));
    assert_eq!(rows[0], row(&["", "", "1"]));

    // Moving among a short row's missing fields leaves it untouched.
    let mut headers = row(&["a", "b", "c"]);
    let mut rows = vec![row(&["1"])];
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut rows,
        3,
        2
    ));
    assert_eq!(rows[0], row(&["1"]));
}

#[test]
fn test_apply_edit_in_extends_short_row_only_for_a_value() {
    let mut headers = row(&["a", "b", "c"]);
    let mut rows = vec![row(&["1"])];
    CsvTableDelegate::apply_edit_in(
        &mut headers,
        &mut rows,
        EditTarget::Cell(0, 2),
        String::new(),
    );
    assert_eq!(rows[0], row(&["1"]));

    CsvTableDelegate::apply_edit_in(
        &mut headers,
        &mut rows,
        EditTarget::Cell(0, 2),
        "x".to_string(),
    );
    assert_eq!(rows[0], row(&["1", "", "x"]));
}

#[test]
fn test_edits_round_trip_through_serialize_csv() {
    let (mut headers, mut rows) = sample();
    CsvTableDelegate::insert_row_below_in(&headers, &mut rows, Some(0));
    CsvTableDelegate::apply_edit_in(
        &mut headers,
        &mut rows,
        EditTarget::Cell(1, 0),
        "5".to_string(),
    );
    assert_eq!(
        serialize_csv(&headers, &rows, &CsvDialect::default()),
        Ok("a,b\n1,2\n5,\n3,4\n".to_string())
    );
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_edit_dialog_does_not_open_on_a_read_only_buffer(cx: &mut gpui_kit::TestAppContext) {
    use crate::fulgur::languages::supported_languages::SupportedLanguage;
    use crate::test_support::{has_active_dialog, setup_fulgur_with_root};

    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let table = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.language = SupportedLanguage::Csv;
                editor.csv_delimiter = b',';
                editor.content.update(cx, |state, cx| {
                    state.set_value("a,b\n1,2", window, cx);
                });
                editor.make_read_only(cx);
                editor.ensure_csv_table(window, cx);
                editor.csv_table.clone().expect("expected a CSV table")
            })
            .expect("expected an active editor tab")
        })
    });

    visual_cx.update(|window, cx| {
        table.update(cx, |state, cx| {
            state
                .delegate_mut()
                .open_edit_dialog(EditTarget::Cell(0, 0), window, cx);
        });
    });
    visual_cx.run_until_parked();

    assert!(!has_active_dialog(&mut visual_cx));
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_table_edits_are_undone_one_at_a_time(cx: &mut gpui_kit::TestAppContext) {
    use crate::fulgur::languages::supported_languages::SupportedLanguage;
    use crate::test_support::setup_fulgur_with_root;
    use gpui_kit::component::input::Undo;

    let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
    let (content, table) = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.language = SupportedLanguage::Csv;
                editor.csv_delimiter = b',';
                editor.content.update(cx, |state, cx| {
                    state.set_value("a,b\n1,2", window, cx);
                });
                editor.ensure_csv_table(window, cx);
                let table = editor.csv_table.clone().expect("expected a CSV table");
                (editor.content.clone(), table)
            })
            .expect("expected an active editor tab")
        })
    });
    let buffer_text = |visual_cx: &mut gpui_kit::VisualTestContext| {
        visual_cx.update(|_, cx| content.read(cx).text().to_string())
    };
    let table_is_kept = |visual_cx: &mut gpui_kit::VisualTestContext| {
        visual_cx.update(|_, cx| {
            fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .is_some_and(|editor| editor.csv_table.is_some())
        })
    };

    let original = buffer_text(&mut visual_cx);
    visual_cx.update(|window, cx| {
        table.update(cx, |state, cx| {
            state.delegate_mut().insert_row_below(window, cx);
        });
    });
    visual_cx.run_until_parked();
    let after_insert = buffer_text(&mut visual_cx);
    visual_cx.update(|window, cx| {
        table.update(cx, |state, cx| {
            state.delegate_mut().insert_column_after(window, cx);
        });
    });
    visual_cx.run_until_parked();
    let after_column = buffer_text(&mut visual_cx);
    assert_ne!(original, after_insert);
    assert_ne!(after_insert, after_column);
    assert!(
        table_is_kept(&mut visual_cx),
        "the table's own commits must not be mistaken for external edits"
    );

    visual_cx.update(|window, cx| content.update(cx, |state, cx| state.focus(window, cx)));
    visual_cx.dispatch_action(Undo);
    visual_cx.run_until_parked();
    assert_eq!(
        buffer_text(&mut visual_cx),
        after_insert,
        "one undo must revert only the last table edit"
    );
    assert!(
        !table_is_kept(&mut visual_cx),
        "an undo is an external change, so the table must be rebuilt"
    );

    visual_cx.dispatch_action(Undo);
    visual_cx.run_until_parked();
    assert_eq!(
        buffer_text(&mut visual_cx),
        original,
        "the earlier table edit must still be in the undo history"
    );
}
