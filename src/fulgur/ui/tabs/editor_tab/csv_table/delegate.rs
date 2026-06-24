//! The `TableDelegate` implementation that renders the CSV grid.

use gpui::{
    App, ClickEvent, Context, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use gpui_component::{
    ActiveTheme,
    table::{Column, TableDelegate, TableState},
};

use super::{CsvTableDelegate, EditTarget};

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
