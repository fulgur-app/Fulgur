use crate::fulgur::{
    Fulgur, editor_tab, languages::supported_languages::SupportedLanguage, tab::Tab, ui,
};
use gpui::{
    AnyElement, Context, DismissEvent, Entity, Focusable, InteractiveElement, IntoElement,
    MouseButton, MouseDownEvent, ParentElement, SharedString, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, WindowExt,
    input::{Input, InputState},
    menu::PopupMenu,
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    table::{DataTable, TableState},
    text::TextView,
    v_flex,
};

impl Fulgur {
    /// Handle a right-click in the editor area to show a custom context menu.
    ///
    /// Called during the capture phase so propagation can be stopped before
    /// the editor's built-in context menu fires.
    ///
    /// ### Arguments
    /// - `event`: The mouse-down event
    /// - `window`: The window context
    /// - `cx`: The application context
    fn on_editor_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Right {
            return;
        }
        cx.stop_propagation();

        let Some(Tab::Editor(editor_tab)) = self.active_tab(cx) else {
            return;
        };
        let active_tab_id = editor_tab.id;
        let editor_focus = editor_tab.content.focus_handle(cx);
        let has_file = editor_tab.file_path().is_some();
        let position = event.position;

        let menu = PopupMenu::build(window, cx, {
            let editor_focus = editor_focus.clone();
            move |mut menu, _window, _cx| {
                menu = menu.action_context(editor_focus);
                if has_file {
                    menu = menu
                        .menu(
                            crate::fulgur::ui::components_utils::reveal_in_file_manager_label(),
                            Box::new(ui::tabs::tab_bar::ShowInFileManager(active_tab_id)),
                        )
                        .separator();
                }
                menu.menu("Cut", Box::new(gpui_component::input::Cut))
                    .menu("Copy", Box::new(gpui_component::input::Copy))
                    .menu("Paste", Box::new(gpui_component::input::Paste))
                    .separator()
                    .menu("Select All", Box::new(gpui_component::input::SelectAll))
            }
        });

        let subscription = cx.subscribe_in(
            &menu,
            window,
            |this: &mut Self, _, _: &DismissEvent, _, cx| {
                this.editor_context_menu = None;
                this.editor_context_menu_subscription = None;
                cx.notify();
            },
        );

        self.editor_context_menu = Some((position, menu));
        self.editor_context_menu_subscription = Some(subscription);
        cx.notify();
    }

    /// Render the content area (editor or settings)
    ///
    /// ### Arguments
    /// - `active_tab_index`: The index of the active tab (if any)
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered content area element (wrapped in `AnyElement`)
    pub(super) fn render_content_area(
        &mut self,
        active_tab_index: Option<usize>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        enum ActiveTabRenderData {
            Editor {
                language: SupportedLanguage,
                show_markdown_preview: bool,
                content: Entity<InputState>,
                csv_view_mode: editor_tab::CsvViewMode,
                csv_table: Option<Entity<TableState<editor_tab::CsvTableDelegate>>>,
                log_view: bool,
                log_content: Option<Entity<InputState>>,
            },
            Settings,
            MarkdownPreview {
                content: Entity<InputState>,
                view_state: Entity<gpui_component::text::TextViewState>,
            },
        }

        // A CSV tab in table mode needs its grid (re)built from the canonical
        // text before we snapshot the tab read-only below. When the parse is
        // lossy, `ensure_csv_table` falls back to text mode and returns a
        // warning to surface to the user.
        let csv_table_warning = if let Some(active_index) = active_tab_index
            && let Some(tab_entity) = self.tabs.get(active_index).cloned()
        {
            tab_entity.update(cx, |tab, cx| match tab {
                Tab::Editor(editor_tab)
                    if editor_tab.language == SupportedLanguage::Csv
                        && editor_tab.csv_view_mode == editor_tab::CsvViewMode::Table =>
                {
                    editor_tab.ensure_csv_table(window, cx)
                }
                _ => None,
            })
        } else {
            None
        };
        if let Some(message) = csv_table_warning {
            window.push_notification((NotificationType::Warning, SharedString::from(message)), cx);
        }

        let tabs_ref = &self.tabs;
        let active_tab = active_tab_index.and_then(|active_index| {
            tabs_ref.get(active_index).map(|tab| match tab.read(cx) {
                Tab::Editor(editor_tab) => ActiveTabRenderData::Editor {
                    language: editor_tab.language,
                    show_markdown_preview: editor_tab.show_markdown_preview,
                    content: editor_tab.content.clone(),
                    csv_view_mode: editor_tab.csv_view_mode,
                    csv_table: editor_tab.csv_table.clone(),
                    log_view: editor_tab.log_view,
                    log_content: editor_tab.log_content.clone(),
                },
                Tab::Settings(_) => ActiveTabRenderData::Settings,
                Tab::MarkdownPreview(preview_tab) => ActiveTabRenderData::MarkdownPreview {
                    content: tabs_ref
                        .iter()
                        .find_map(|t| match t.read(cx) {
                            Tab::Editor(editor_tab)
                                if editor_tab.id == preview_tab.source_tab_id =>
                            {
                                Some(editor_tab.content.clone())
                            }
                            _ => None,
                        })
                        .unwrap_or_else(|| preview_tab.content.clone()),
                    view_state: preview_tab.view_state.clone(),
                },
            })
        });

        if let Some(tab) = active_tab {
            match tab {
                ActiveTabRenderData::Editor {
                    language,
                    show_markdown_preview,
                    content,
                    csv_view_mode,
                    csv_table,
                    log_view,
                    log_content,
                } => {
                    if log_view && let Some(log_content) = log_content {
                        let log_input = Input::new(&log_content)
                            .disabled(true)
                            .bordered(false)
                            .p_0()
                            .h_full()
                            .font_family(self.settings.editor_settings.font_family.clone())
                            .text_size(px(self.settings.editor_settings.font_size))
                            .focus_bordered(false);
                        return v_flex()
                            .w_full()
                            .flex_1()
                            .child(log_input)
                            .into_any_element();
                    }
                    if language == SupportedLanguage::Csv
                        && csv_view_mode == editor_tab::CsvViewMode::Table
                        && let Some(table) = csv_table
                    {
                        return v_flex()
                            .w_full()
                            .flex_1()
                            .child(DataTable::new(&table).bordered(true).stripe(true))
                            .into_any_element();
                    }
                    let editor_input = Input::new(&content)
                        .bordered(false)
                        .p_0()
                        .h_full()
                        .font_family(self.settings.editor_settings.font_family.clone())
                        .text_size(px(self.settings.editor_settings.font_size))
                        .focus_bordered(false);
                    let capture_right_click =
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.on_editor_right_click(event, window, cx);
                        });
                    if language == SupportedLanguage::Markdown
                        && show_markdown_preview
                        && self.settings.editor_settings.markdown_settings.preview_mode
                            == crate::fulgur::settings::MarkdownPreviewMode::Panel
                    {
                        // Reading the content entity here tracks it for this
                        // window, so edits re-render the panel automatically.
                        let preview_text = content.read(cx).value();
                        return v_flex()
                            .w_full()
                            .flex_1()
                            .child(
                                h_resizable("markdown-preview-container")
                                    .child(
                                        resizable_panel().child(
                                            div()
                                                .id("markdown-editor")
                                                .size_full()
                                                .capture_any_mouse_down(capture_right_click)
                                                .child(editor_input),
                                        ),
                                    )
                                    .child(
                                        resizable_panel().child(
                                            TextView::markdown("markdown-preview", preview_text)
                                                .flex_none()
                                                .py_0()
                                                .px_2()
                                                .scrollable(true)
                                                .selectable(true)
                                                .bg(cx.theme().muted),
                                        ),
                                    ),
                            )
                            .into_any_element();
                    }
                    return v_flex()
                        .w_full()
                        .flex_1()
                        .capture_any_mouse_down(capture_right_click)
                        .child(editor_input)
                        .into_any_element();
                }
                ActiveTabRenderData::Settings => {
                    return v_flex()
                        .id("settings-tab-scrollable")
                        .w_full()
                        .flex_1()
                        .overflow_y_scrollbar()
                        .child(self.render_settings(window, cx))
                        .into_any_element();
                }
                ActiveTabRenderData::MarkdownPreview {
                    content,
                    view_state,
                } => {
                    let preview_text = content.read(cx).value();
                    view_state.update(cx, |state, cx| {
                        state.set_text(preview_text.as_ref(), cx);
                    });
                    return v_flex()
                        .w_full()
                        .flex_1()
                        .child(
                            TextView::new(&view_state)
                                .py_2()
                                .px_4()
                                .scrollable(true)
                                .selectable(true),
                        )
                        .into_any_element();
                }
            }
        }
        v_flex().w_full().flex_1().into_any_element()
    }
}
