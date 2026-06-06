//! The bottom toolbar shown for CSV tabs in table view.

use gpui::{Context, Div, Entity, ParentElement, Styled, Window, div};
use gpui_component::{ActiveTheme, h_flex, table::TableState};

use crate::fulgur::Fulgur;
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::ui::bars::search_bar::search_bar_button_factory;
use crate::fulgur::ui::components_utils::SEARCH_BAR_HEIGHT;
use crate::fulgur::ui::icons::CustomIcon;
use crate::fulgur::ui::tabs::editor_tab::{CsvTableDelegate, CsvViewMode};

impl Fulgur {
    /// Render the CSV structural-edit toolbar for the active tab.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Div)`: The toolbar when the active tab is a CSV tab in table view
    /// - `None`: Otherwise
    pub fn render_csv_toolbar(&self, cx: &mut Context<Self>) -> Option<Div> {
        let editor = self.get_active_editor_tab()?;
        if editor.language != SupportedLanguage::Csv || editor.csv_view_mode != CsvViewMode::Table {
            return None;
        }

        let border = cx.theme().border;
        Some(
            div()
                .flex()
                .items_center()
                .w_full()
                .h(SEARCH_BAR_HEIGHT)
                .bg(cx.theme().tab_bar)
                .border_t_1()
                .border_color(border)
                .child(
                    h_flex()
                        .border_r_1()
                        .border_color(border)
                        .child(
                            search_bar_button_factory(
                                "csv-add-row-above",
                                "Add row above",
                                CustomIcon::AddRowAbove,
                                border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.csv_insert_row_above(window, cx);
                                },
                            )),
                        )
                        .child(
                            search_bar_button_factory(
                                "csv-add-row-below",
                                "Add row below",
                                CustomIcon::AddRowBelow,
                                border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.csv_insert_row_below(window, cx);
                                },
                            )),
                        )
                        .child(
                            search_bar_button_factory(
                                "csv-delete-row",
                                "Delete row",
                                CustomIcon::DeleteRow,
                                border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.csv_delete_row(window, cx);
                                },
                            )),
                        ),
                )
                .child(
                    h_flex()
                        .child(
                            search_bar_button_factory(
                                "csv-add-column-before",
                                "Add column before",
                                CustomIcon::AddColumnBefore,
                                border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.csv_insert_column_before(window, cx);
                                },
                            )),
                        )
                        .child(
                            search_bar_button_factory(
                                "csv-add-column-after",
                                "Add column after",
                                CustomIcon::AddColumnAfter,
                                border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.csv_insert_column_after(window, cx);
                                },
                            )),
                        )
                        .child(
                            search_bar_button_factory(
                                "csv-delete-column",
                                "Delete column",
                                CustomIcon::DeleteColumn,
                                border,
                            )
                            .on_click(cx.listener(
                                |this, _, window, cx| {
                                    this.csv_delete_column(window, cx);
                                },
                            )),
                        ),
                ),
        )
    }

    /// Toggle the active CSV tab between the table and text views.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn toggle_csv_view_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(editor) = self.get_active_editor_tab_mut() else {
            return;
        };
        if editor.language != SupportedLanguage::Csv {
            return;
        }
        editor.csv_view_mode = match editor.csv_view_mode {
            CsvViewMode::Table => CsvViewMode::Text,
            CsvViewMode::Text => CsvViewMode::Table,
        };
        if editor.csv_view_mode == CsvViewMode::Table {
            editor.ensure_csv_table(window, cx);
        }
        cx.notify();
    }

    /// Return the active tab's CSV table state, if it is a CSV table tab.
    ///
    /// ### Returns
    /// - `Some(Entity<TableState<CsvTableDelegate>>)`: The built table state
    /// - `None`: If the active tab is not a CSV tab with a built table
    fn active_csv_table(&self) -> Option<Entity<TableState<CsvTableDelegate>>> {
        let editor = self.get_active_editor_tab()?;
        if editor.language != SupportedLanguage::Csv {
            return None;
        }
        editor.csv_table.clone()
    }

    /// Insert a row above the selected cell's row (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    fn csv_insert_row_above(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(table) = self.active_csv_table() else {
            return;
        };
        table.update(cx, |state, cx| {
            let selected = state.selected_cell();
            state.delegate_mut().insert_row_above(selected, window, cx);
        });
    }

    /// Insert a row below the selected cell's row (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    fn csv_insert_row_below(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(table) = self.active_csv_table() else {
            return;
        };
        table.update(cx, |state, cx| {
            let selected = state.selected_cell();
            state.delegate_mut().insert_row_below(selected, window, cx);
        });
    }

    /// Delete the selected cell's row (or the last row).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    fn csv_delete_row(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(table) = self.active_csv_table() else {
            return;
        };
        table.update(cx, |state, cx| {
            let selected = state.selected_cell();
            state.delegate_mut().delete_row(selected, window, cx);
        });
    }

    /// Insert a column before the selected cell's column (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    fn csv_insert_column_before(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(table) = self.active_csv_table() else {
            return;
        };
        table.update(cx, |state, cx| {
            let selected = state.selected_cell();
            state
                .delegate_mut()
                .insert_column_before(selected, window, cx);
        });
    }

    /// Insert a column after the selected cell's column (or at the end).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    fn csv_insert_column_after(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(table) = self.active_csv_table() else {
            return;
        };
        table.update(cx, |state, cx| {
            let selected = state.selected_cell();
            state
                .delegate_mut()
                .insert_column_after(selected, window, cx);
        });
    }

    /// Delete the selected cell's column (or the last column).
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    fn csv_delete_column(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(table) = self.active_csv_table() else {
            return;
        };
        table.update(cx, |state, cx| {
            let selected = state.selected_cell();
            state.delegate_mut().delete_column(selected, window, cx);
        });
    }
}
