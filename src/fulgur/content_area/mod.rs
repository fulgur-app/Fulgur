mod context_menu;
mod markdown_links;
mod markdown_preview;
/// Rendering regressions for the Markdown preview.
#[cfg(all(test, feature = "gpui-test-support"))]
mod markdown_preview_tests;

use crate::fulgur::ui::tabs::markdown_preview_source::MarkdownPreviewSource;
use crate::fulgur::ui::tabs::tab::TabId;
use crate::fulgur::{
    Fulgur, editor_tab, languages::supported_languages::SupportedLanguage, tab::Tab,
};
use gpui_kit::base::text::TextView;
use gpui_kit::component::{
    ActiveTheme, WindowExt,
    input::{Editor, EditorState},
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    table::{DataTable, TableState},
    v_flex,
};
use gpui_kit::{
    AnyElement, Context, Entity, InteractiveElement, IntoElement, MouseDownEvent, ParentElement,
    SharedString, Styled, Window, div, px,
};

impl Fulgur {
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
                tab: Entity<Tab>,
                tab_id: TabId,
                language: SupportedLanguage,
                show_markdown_preview: bool,
                large_file: bool,
                content: Entity<EditorState>,
                csv_view_mode: editor_tab::CsvViewMode,
                csv_table: Option<Entity<TableState<editor_tab::CsvTableDelegate>>>,
                blocks_user_edits: bool,
            },
            Settings,
            MarkdownPreview {
                preview: Entity<MarkdownPreviewSource>,
            },
        }

        // A CSV tab in table mode needs its grid built before we snapshot the
        // tab read-only below. This only parses when the table is missing (first
        // show, or dropped by an external edit). When the parse is lossy,
        // `ensure_csv_table` falls back to text mode and returns a warning to
        // surface to the user.
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
                    tab: tab.clone(),
                    tab_id: editor_tab.id,
                    language: editor_tab.language,
                    show_markdown_preview: editor_tab.show_markdown_preview,
                    large_file: editor_tab.large_file,
                    content: editor_tab.content.clone(),
                    csv_view_mode: editor_tab.csv_view_mode,
                    csv_table: editor_tab.csv_table.clone(),
                    blocks_user_edits: editor_tab.blocks_user_edits(),
                },
                Tab::Settings(_) => ActiveTabRenderData::Settings,
                Tab::MarkdownPreview(preview_tab) => ActiveTabRenderData::MarkdownPreview {
                    preview: preview_tab.preview.clone(),
                },
            })
        });

        if let Some(tab) = active_tab {
            match tab {
                ActiveTabRenderData::Editor {
                    tab,
                    tab_id,
                    language,
                    show_markdown_preview,
                    large_file,
                    content,
                    csv_view_mode,
                    csv_table,
                    blocks_user_edits,
                } => {
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

                    let editor_input = Editor::new(&content)
                        .aria_label("Editor")
                        .readonly(blocks_user_edits)
                        .bordered(false)
                        .p_0()
                        .h_full()
                        .font_family(self.settings.editor_settings.font_family.clone())
                        .text_size(px(self.settings.editor_settings.font_size));
                    let capture_right_click =
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.on_editor_right_click(event, window, cx);
                        });
                    let shows_markdown_panel = language == SupportedLanguage::Markdown
                        && show_markdown_preview
                        && !large_file
                        && self.settings.editor_settings.markdown_settings.preview_mode
                            == crate::fulgur::settings::MarkdownPreviewMode::Panel;
                    if !shows_markdown_panel {
                        self.release_markdown_panel_preview_of(tab_id, cx);
                    }
                    if shows_markdown_panel
                        && let Some(markdown_preview) = self.ensure_markdown_panel_preview(&tab, cx)
                    {
                        let (preview_state, base_dir) = {
                            let markdown_preview = markdown_preview.read(cx);
                            (
                                markdown_preview.view_state(),
                                markdown_preview
                                    .base_dir()
                                    .map(std::path::Path::to_path_buf),
                            )
                        };
                        let link_tooltip = Self::markdown_link_tooltip(base_dir.clone());
                        let link_handler = Self::markdown_link_handler(base_dir, cx);
                        let text_view = TextView::new(&preview_state)
                            .markdown_extensions(self.markdown_preview_extensions())
                            .flex_none()
                            .py_0()
                            .px_2()
                            .scrollable(true)
                            .selectable(true)
                            .on_link_click(link_handler)
                            .link_tooltip(link_tooltip)
                            .code_block_actions(|code_block, _window, _cx| {
                                Self::markdown_code_block_copy(code_block.code())
                            })
                            .table_actions(Self::markdown_table_actions);
                        let preview = self
                            .layout_markdown_preview(Self::with_markdown_task_toggles(
                                text_view,
                                &markdown_preview,
                                cx,
                            ))
                            .bg(cx.theme().muted)
                            .into_any_element();
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
                                        resizable_panel()
                                            .child(self.wrap_markdown_preview(preview, cx)),
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
                ActiveTabRenderData::MarkdownPreview { preview } => {
                    let (view_state, base_dir) = {
                        let preview = preview.read(cx);
                        (
                            preview.view_state(),
                            preview.base_dir().map(std::path::Path::to_path_buf),
                        )
                    };
                    let link_tooltip = Self::markdown_link_tooltip(base_dir.clone());
                    let link_handler = Self::markdown_link_handler(base_dir, cx);
                    let text_view = TextView::new(&view_state)
                        .markdown_extensions(self.markdown_preview_extensions())
                        .py_2()
                        .px_4()
                        .scrollable(true)
                        .selectable(true)
                        .on_link_click(link_handler)
                        .link_tooltip(link_tooltip)
                        .code_block_actions(|code_block, _window, _cx| {
                            Self::markdown_code_block_copy(code_block.code())
                        })
                        .table_actions(Self::markdown_table_actions);
                    let preview = self
                        .layout_markdown_preview(Self::with_markdown_task_toggles(
                            text_view, &preview, cx,
                        ))
                        .into_any_element();
                    return v_flex()
                        .w_full()
                        .flex_1()
                        .child(self.wrap_markdown_preview(preview, cx))
                        .into_any_element();
                }
            }
        }
        v_flex().w_full().flex_1().into_any_element()
    }
}
