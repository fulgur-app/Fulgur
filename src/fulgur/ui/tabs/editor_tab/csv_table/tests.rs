use super::{CsvSelection, CsvTableDelegate, EditTarget};
use crate::fulgur::files::csv_support::{CsvDialect, header_record, serialize_csv};

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
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2", "3"]), row(&["4", "5"]), row(&["6"])];
    CsvTableDelegate::insert_column_at_in(&mut headers, &mut header_len, &mut rows, 1);
    assert_eq!(headers, row(&["a", "", "b", "c"]));
    assert_eq!(rows[0], row(&["1", "", "2", "3"]));
    assert_eq!(rows[1], row(&["4", "", "5"]));
    // The new column falls in the short row's missing fields.
    assert_eq!(rows[2], row(&["6"]));

    // Appending a column grows full rows only.
    CsvTableDelegate::insert_column_at_in(&mut headers, &mut header_len, &mut rows, 4);
    assert_eq!(rows[0], row(&["1", "", "2", "3", ""]));
    assert_eq!(rows[1], row(&["4", "", "5"]));
}

#[test]
fn test_insert_column_before_and_after_resolve_selection() {
    let mut headers = row(&["a", "b"]);
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2"])];
    CsvTableDelegate::insert_column_before_in(&mut headers, &mut header_len, &mut rows, Some(1));
    assert_eq!(headers, row(&["a", "", "b"]));
    assert_eq!(rows[0], row(&["1", "", "2"]));

    let mut headers = row(&["a", "b"]);

    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2"])];
    CsvTableDelegate::insert_column_after_in(&mut headers, &mut header_len, &mut rows, Some(0));
    assert_eq!(headers, row(&["a", "", "b"]));
    assert_eq!(rows[0], row(&["1", "", "2"]));

    let mut headers = row(&["a", "b"]);

    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2"])];
    CsvTableDelegate::insert_column_before_in(&mut headers, &mut header_len, &mut rows, None);
    assert_eq!(headers, row(&["a", "b", ""]));
    assert_eq!(rows[0], row(&["1", "2", ""]));
}

#[test]
fn test_delete_column_in_selected_last_and_empty() {
    let mut headers = row(&["a", "b", "c"]);
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2", "3"])];
    assert!(CsvTableDelegate::delete_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        Some(1)
    ));
    assert_eq!(headers, row(&["a", "c"]));
    assert_eq!(rows[0], row(&["1", "3"]));

    // None deletes the last column.
    let mut headers = row(&["a", "b", "c"]);
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2", "3"])];
    assert!(CsvTableDelegate::delete_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        None
    ));
    assert_eq!(headers, row(&["a", "b"]));
    assert_eq!(rows[0], row(&["1", "2"]));

    let mut headers: Vec<String> = Vec::new();

    let mut header_len = headers.len();
    let mut rows: Vec<Vec<String>> = Vec::new();
    assert!(!CsvTableDelegate::delete_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        None
    ));
}

#[test]
fn test_move_column_in_reorders_header_and_cells() {
    let mut headers = row(&["a", "b", "c"]);
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1", "2", "3"])];
    // Grid index 1 is data column 0; move it after data column 2.
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
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
    let mut header_len = headers.len();

    // The row-number column (grid index 0) cannot be moved.
    assert!(!CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        0,
        2
    ));
    // Out-of-bounds source.
    assert!(!CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        9,
        1
    ));
    // Same source and destination.
    assert!(!CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
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
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1"]), row(&["2", "3"]), row(&["4", "", "5"])];
    // Move data column 2 to the front.
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
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
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1"])];
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        1,
        3
    ));
    assert_eq!(rows[0], row(&["", "", "1"]));

    // Moving among a short row's missing fields leaves it untouched.
    let mut headers = row(&["a", "b", "c"]);
    let mut header_len = headers.len();
    let mut rows = vec![row(&["1"])];
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
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

/// A model parsed from `a,b\n1,2,3\n`: the header record has two fields and
/// is padded to the three-field row.
fn padded_header_sample() -> (Vec<String>, usize, Vec<Vec<String>>) {
    (row(&["a", "b", ""]), 2, vec![row(&["1", "2", "3"])])
}

/// Serialize a model, writing only the header record's fields.
fn serialize_model(headers: &[String], header_len: usize, rows: &[Vec<String>]) -> String {
    serialize_csv(
        header_record(headers, header_len),
        rows,
        &CsvDialect::default(),
    )
    .expect("serialization succeeds")
}

#[test]
fn test_cell_edit_does_not_pad_short_header() {
    let (mut headers, header_len, mut rows) = padded_header_sample();
    CsvTableDelegate::apply_edit_in(
        &mut headers,
        &mut rows,
        EditTarget::Cell(0, 2),
        "x".to_string(),
    );
    assert_eq!(serialize_model(&headers, header_len, &rows), "a,b\n1,2,x\n");
}

#[test]
fn test_header_typed_into_padded_column_is_written() {
    let (mut headers, header_len, mut rows) = padded_header_sample();
    CsvTableDelegate::apply_edit_in(
        &mut headers,
        &mut rows,
        EditTarget::Header(2),
        "c".to_string(),
    );
    assert_eq!(
        serialize_model(&headers, header_len, &rows),
        "a,b,c\n1,2,3\n"
    );
}

#[test]
fn test_column_insert_and_delete_keep_short_header_in_step() {
    let (mut headers, mut header_len, mut rows) = padded_header_sample();
    CsvTableDelegate::insert_column_at_in(&mut headers, &mut header_len, &mut rows, 0);
    assert_eq!(header_len, 3);
    assert_eq!(
        serialize_model(&headers, header_len, &rows),
        ",a,b\n,1,2,3\n"
    );

    // Appending past the header record leaves it short.
    let (mut headers, mut header_len, mut rows) = padded_header_sample();
    CsvTableDelegate::insert_column_at_in(&mut headers, &mut header_len, &mut rows, 3);
    assert_eq!(
        serialize_model(&headers, header_len, &rows),
        "a,b\n1,2,3,\n"
    );

    // Deleting the padded column leaves the header record untouched.
    let (mut headers, mut header_len, mut rows) = padded_header_sample();
    assert!(CsvTableDelegate::delete_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        Some(2)
    ));
    assert_eq!(serialize_model(&headers, header_len, &rows), "a,b\n1,2\n");

    // Deleting a real header field shortens the header record.
    let (mut headers, mut header_len, mut rows) = padded_header_sample();
    assert!(CsvTableDelegate::delete_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        Some(0)
    ));
    assert_eq!(serialize_model(&headers, header_len, &rows), "b\n2,3\n");
}

#[test]
fn test_column_move_keeps_short_header_in_step() {
    // Moving the padded column to the front brings it inside the header record.
    let (mut headers, mut header_len, mut rows) = padded_header_sample();
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        3,
        1
    ));
    assert_eq!(
        serialize_model(&headers, header_len, &rows),
        ",a,b\n3,1,2\n"
    );

    // Moving a named header past the padding keeps it, and the gap before it.
    let (mut headers, mut header_len, mut rows) = padded_header_sample();
    assert!(CsvTableDelegate::move_column_in(
        &mut headers,
        &mut header_len,
        &mut rows,
        1,
        3
    ));
    assert_eq!(
        serialize_model(&headers, header_len, &rows),
        "b,,a\n2,3,1\n"
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

/// Open a window whose active tab shows `source` as a CSV table.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `source`: The CSV document
///
/// ### Returns
/// - `(Entity<EditorState>, Entity<TableState<CsvTableDelegate>>, VisualTestContext)`: The tab's
///   buffer, its table and the visual test context
#[cfg(feature = "gpui-test-support")]
fn open_csv_table(
    cx: &mut gpui_kit::TestAppContext,
    source: &'static str,
) -> (
    gpui_kit::Entity<gpui_kit::component::input::EditorState>,
    gpui_kit::Entity<gpui_kit::component::table::TableState<CsvTableDelegate>>,
    gpui_kit::VisualTestContext,
) {
    use crate::fulgur::languages::supported_languages::SupportedLanguage;

    let (fulgur, mut visual_cx) = crate::test_support::setup_fulgur_with_root(cx);
    let (content, table) = visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.language = SupportedLanguage::Csv;
                editor.csv_delimiter = b',';
                editor.content.update(cx, |state, cx| {
                    state.set_value(source, window, cx);
                });
                editor.ensure_csv_table(window, cx);
                let table = editor.csv_table.clone().expect("expected a CSV table");
                (editor.content.clone(), table)
            })
            .expect("expected an active editor tab")
        })
    });
    (content, table, visual_cx)
}

/// Open the edit dialog on a cell of the first row, press keys in it and confirm it.
///
/// ### Arguments
/// - `table`: The table owning the cell
/// - `column`: The data column of the cell
/// - `keystrokes`: The keys pressed before the dialog is confirmed
/// - `visual_cx`: The visual test context driving the window
#[cfg(feature = "gpui-test-support")]
fn edit_first_row_cell(
    table: &gpui_kit::Entity<gpui_kit::component::table::TableState<CsvTableDelegate>>,
    column: usize,
    keystrokes: &str,
    visual_cx: &mut gpui_kit::VisualTestContext,
) {
    use crate::test_support::has_active_dialog;

    visual_cx.update(|window, cx| {
        table.update(cx, |state, cx| {
            state
                .delegate_mut()
                .open_edit_dialog(EditTarget::Cell(0, column), window, cx);
        });
    });
    visual_cx.run_until_parked();
    assert!(has_active_dialog(visual_cx));
    visual_cx.simulate_keystrokes(keystrokes);
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();
    assert!(!has_active_dialog(visual_cx));
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_confirming_an_untouched_edit_leaves_the_buffer_untouched(
    cx: &mut gpui_kit::TestAppContext,
) {
    let source = "a,b\n\"x\ty\",\"line 1\nline 2\"";
    let (content, table, mut visual_cx) = open_csv_table(cx, source);
    for column in [0, 1] {
        edit_first_row_cell(&table, column, "", &mut visual_cx);
        assert_eq!(
            visual_cx.update(|_, cx| content.read(cx).text().to_string()),
            source,
            "confirming column {column} without typing must not rewrite the cell"
        );
    }
}

#[cfg(feature = "gpui-test-support")]
#[gpui_kit::test]
fn test_editing_a_cell_keeps_its_tabs_and_line_breaks(cx: &mut gpui_kit::TestAppContext) {
    let (content, table, mut visual_cx) = open_csv_table(cx, "a,b\nx\ty,\"line 1\nline 2\"");
    edit_first_row_cell(&table, 0, "z", &mut visual_cx);
    edit_first_row_cell(&table, 1, "shift-enter z", &mut visual_cx);
    assert_eq!(
        visual_cx.update(|_, cx| content.read(cx).text().to_string()),
        "a,b\nx\tyz,\"line 1\nline 2\nz\""
    );
}
