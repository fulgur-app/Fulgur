//! The bottom toolbar shown for CSV tabs in table view, rendered as its own entity.

use gpui::{
    App, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled, WeakEntity,
    Window, div,
};
use gpui_component::{
    ActiveTheme, WindowExt, h_flex, notification::NotificationType, table::TableState,
};

use crate::fulgur::Fulgur;
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::ui::bars::search_bar::search_bar_button_factory;
use crate::fulgur::ui::components_utils::SEARCH_BAR_HEIGHT;
use crate::fulgur::ui::icons::CustomIcon;
use crate::fulgur::ui::tabs::editor_tab::{CsvTableDelegate, CsvViewMode};

/// The signature shared by all `CsvTableDelegate` structural-edit methods
type CsvTableEdit =
    fn(&mut CsvTableDelegate, &mut Window, &mut Context<TableState<CsvTableDelegate>>);

/// The CSV structural-edit toolbar, rendered as its own entity
pub(crate) struct CsvToolbar {
    fulgur: WeakEntity<Fulgur>,
}

impl CsvToolbar {
    /// Create a new CSV toolbar view
    ///
    /// ### Arguments
    /// - `fulgur`: Weak handle to the owning window entity the bar reads the active tab from
    ///
    /// ### Returns
    /// - `CsvToolbar`: The new CSV toolbar view
    pub(crate) fn new(fulgur: WeakEntity<Fulgur>) -> Self {
        Self { fulgur }
    }

    /// Get the active tab's CSV table state from the owning window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<TableState<CsvTableDelegate>>)`: The built table state
    /// - `None`: If the window is gone or the active tab is not a CSV tab with a built table
    fn active_csv_table(&self, cx: &App) -> Option<Entity<TableState<CsvTableDelegate>>> {
        let fulgur = self.fulgur.upgrade()?;
        let editor = fulgur.read(cx).get_active_editor_tab(cx)?;
        if editor.language != SupportedLanguage::Csv {
            return None;
        }
        editor.csv_table.clone()
    }

    /// Apply a structural edit to the active tab's CSV table at its most
    /// recent selection (cell, column header, or row)
    ///
    /// ### Arguments
    /// - `edit`: The `CsvTableDelegate` edit to apply (e.g. `CsvTableDelegate::insert_row_above`)
    /// - `window`: The active window
    /// - `cx`: The application context
    fn edit_active_table(
        &mut self,
        edit: CsvTableEdit,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(table) = self.active_csv_table(cx) else {
            return;
        };
        table.update(cx, |state, cx| {
            edit(state.delegate_mut(), window, cx);
        });
    }
}

impl Fulgur {
    /// Whether the CSV toolbar should be mounted for the active tab
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `bool`: True if the active tab is a CSV tab in table view
    pub(crate) fn csv_toolbar_visible(&self, cx: &gpui::App) -> bool {
        self.get_active_editor_tab(cx).is_some_and(|editor| {
            editor.language == SupportedLanguage::Csv && editor.csv_view_mode == CsvViewMode::Table
        })
    }

    /// Toggle the active CSV tab between the table and text views.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn toggle_csv_view_mode(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let warning = self.update_active_editor_tab(cx, |editor, cx| {
            if editor.language != SupportedLanguage::Csv || editor.large_file {
                return None;
            }
            editor.csv_view_mode = match editor.csv_view_mode {
                CsvViewMode::Table => CsvViewMode::Text,
                CsvViewMode::Text => CsvViewMode::Table,
            };
            cx.notify();
            if editor.csv_view_mode == CsvViewMode::Table {
                editor.ensure_csv_table(window, cx)
            } else {
                None
            }
        });
        let Some(warning) = warning else {
            return;
        };
        if let Some(message) = warning {
            window.push_notification((NotificationType::Warning, SharedString::from(message)), cx);
        }
        cx.notify();
    }
}

impl Render for CsvToolbar {
    /// Render the CSV structural-edit toolbar
    ///
    /// ### Arguments
    /// - `_window`: The window to render the CSV toolbar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered CSV toolbar
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let border = cx.theme().border;
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
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_active_table(CsvTableDelegate::insert_row_above, window, cx);
                        })),
                    )
                    .child(
                        search_bar_button_factory(
                            "csv-add-row-below",
                            "Add row below",
                            CustomIcon::AddRowBelow,
                            border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_active_table(CsvTableDelegate::insert_row_below, window, cx);
                        })),
                    )
                    .child(
                        search_bar_button_factory(
                            "csv-delete-row",
                            "Delete row",
                            CustomIcon::DeleteRow,
                            border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_active_table(CsvTableDelegate::delete_row, window, cx);
                        })),
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
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_active_table(
                                CsvTableDelegate::insert_column_before,
                                window,
                                cx,
                            );
                        })),
                    )
                    .child(
                        search_bar_button_factory(
                            "csv-add-column-after",
                            "Add column after",
                            CustomIcon::AddColumnAfter,
                            border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_active_table(
                                CsvTableDelegate::insert_column_after,
                                window,
                                cx,
                            );
                        })),
                    )
                    .child(
                        search_bar_button_factory(
                            "csv-delete-column",
                            "Delete column",
                            CustomIcon::DeleteColumn,
                            border,
                        )
                        .on_click(cx.listener(|this, _, window, cx| {
                            this.edit_active_table(CsvTableDelegate::delete_column, window, cx);
                        })),
                    ),
            )
    }
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use super::CsvToolbar;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        Fulgur, languages::supported_languages::SupportedLanguage, settings::Settings,
        shared_state::SharedAppState, ui::tabs::editor_tab::CsvTableDelegate,
        window_manager::WindowManager,
    };
    #[cfg(feature = "gpui-test-support")]
    use core::prelude::v1::test;
    #[cfg(feature = "gpui-test-support")]
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::Root;
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    #[cfg(feature = "gpui-test-support")]
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files, None));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");

        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    /// Set up a `Fulgur` window whose active tab is a CSV tab in table view,
    /// and return its CSV toolbar entity.
    #[cfg(feature = "gpui-test-support")]
    fn setup_csv_toolbar(
        cx: &mut TestAppContext,
    ) -> (Entity<Fulgur>, Entity<CsvToolbar>, VisualTestContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.update_active_editor_tab(cx, |editor, cx| {
                    editor.language = SupportedLanguage::Csv;
                    editor.csv_delimiter = b',';
                    editor.content.update(cx, |content, cx| {
                        content.set_value("a,b\n1,2", window, cx);
                    });
                })
                .expect("expected active editor tab");
                this.toggle_csv_view_mode(window, cx);
            });
        });
        let toolbar = visual_cx.update(|_window, cx| fulgur.read(cx).csv_toolbar.clone());
        (fulgur, toolbar, visual_cx)
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_csv_toolbar_visible_only_in_table_view(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(!this.csv_toolbar_visible(cx));

                this.update_active_editor_tab(cx, |editor, cx| {
                    editor.language = SupportedLanguage::Csv;
                    editor.csv_delimiter = b',';
                    editor.content.update(cx, |content, cx| {
                        content.set_value("a,b\n1,2", window, cx);
                    });
                })
                .expect("expected active editor tab");
                assert!(!this.csv_toolbar_visible(cx));

                this.toggle_csv_view_mode(window, cx);
                assert!(this.csv_toolbar_visible(cx));

                this.toggle_csv_view_mode(window, cx);
                assert!(!this.csv_toolbar_visible(cx));
            });
        });
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_edit_active_table_inserts_row_through_toolbar(cx: &mut TestAppContext) {
        let (fulgur, toolbar, mut visual_cx) = setup_csv_toolbar(cx);

        visual_cx.update(|window, cx| {
            toolbar.update(cx, |bar, cx| {
                bar.edit_active_table(CsvTableDelegate::insert_row_below, window, cx);
            });

            let text = fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "a,b\n1,2\n,\n");
        });
    }

    /// Read the active editor tab's CSV table entity.
    #[cfg(feature = "gpui-test-support")]
    fn active_csv_table(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
    ) -> Entity<gpui_component::table::TableState<CsvTableDelegate>> {
        visual_cx.update(|_window, cx| {
            fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .csv_table
                .clone()
                .expect("expected built CSV table")
        })
    }

    /// Read the active editor tab's buffer text.
    #[cfg(feature = "gpui-test-support")]
    fn active_tab_text(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) -> String {
        visual_cx.update(|_window, cx| {
            fulgur
                .read(cx)
                .get_active_editor_tab(cx)
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string()
        })
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_edit_active_table_inserts_column_at_selected_cell(cx: &mut TestAppContext) {
        let (fulgur, toolbar, mut visual_cx) = setup_csv_toolbar(cx);
        let table = active_csv_table(&fulgur, &mut visual_cx);

        visual_cx.update(|_window, cx| {
            table.update(cx, |state, cx| {
                state.set_selected_cell(0, 2, cx);
            });
        });
        visual_cx.run_until_parked();

        visual_cx.update(|window, cx| {
            toolbar.update(cx, |bar, cx| {
                bar.edit_active_table(CsvTableDelegate::insert_column_before, window, cx);
            });
        });

        assert_eq!(active_tab_text(&fulgur, &mut visual_cx), "a,,b\n1,,2\n");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_column_ops_follow_column_header_selection(cx: &mut TestAppContext) {
        let (fulgur, toolbar, mut visual_cx) = setup_csv_toolbar(cx);
        let table = active_csv_table(&fulgur, &mut visual_cx);

        // Grid column 1 is data column 0 ("a"); grid column 0 is the
        // synthetic row-number column.
        visual_cx.update(|_window, cx| {
            table.update(cx, |state, cx| {
                state.set_selected_col(1, cx);
            });
        });
        visual_cx.run_until_parked();

        visual_cx.update(|window, cx| {
            toolbar.update(cx, |bar, cx| {
                bar.edit_active_table(CsvTableDelegate::insert_column_before, window, cx);
            });
        });
        assert_eq!(active_tab_text(&fulgur, &mut visual_cx), ",a,b\n,1,2\n");

        visual_cx.update(|window, cx| {
            toolbar.update(cx, |bar, cx| {
                bar.edit_active_table(CsvTableDelegate::delete_column, window, cx);
            });
        });
        assert_eq!(active_tab_text(&fulgur, &mut visual_cx), "a,b\n1,2\n");
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    #[cfg_attr(
        target_os = "macos",
        ignore = "known upstream a11y panic on gpui TestWindow"
    )]
    fn test_table_survives_its_own_commits(cx: &mut TestAppContext) {
        let (fulgur, toolbar, mut visual_cx) = setup_csv_toolbar(cx);
        let table_before = active_csv_table(&fulgur, &mut visual_cx);

        visual_cx.update(|window, cx| {
            toolbar.update(cx, |bar, cx| {
                bar.edit_active_table(CsvTableDelegate::insert_row_below, window, cx);
            });
        });
        visual_cx.run_until_parked();

        // The next ensure pass (normally triggered by the window render) must
        // keep the table the edit was made through instead of rebuilding it,
        // which would drop the selection and scroll state.
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let warning = this
                    .update_active_editor_tab(cx, |editor, cx| editor.ensure_csv_table(window, cx))
                    .expect("expected active editor tab");
                assert!(warning.is_none());
            });
        });

        let table_after = active_csv_table(&fulgur, &mut visual_cx);
        assert_eq!(table_before.entity_id(), table_after.entity_id());
    }
}
