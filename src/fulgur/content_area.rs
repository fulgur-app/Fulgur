use crate::fulgur::ui::copy_button::CopyButton;
use crate::fulgur::utils::markdown_links::{MarkdownLinkTarget, resolve_markdown_link};
use crate::fulgur::{
    Fulgur, editor_tab, languages::supported_languages::SupportedLanguage, tab::Tab, ui,
};
use gpui::prelude::FluentBuilder;
use gpui::{
    AnyElement, App, AppContext, ClickEvent, Context, DismissEvent, Div, Entity, Focusable,
    InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, SharedString,
    Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, WindowExt, h_flex,
    input::{Editor, EditorState},
    menu::PopupMenu,
    notification::NotificationType,
    resizable::{h_resizable, resizable_panel},
    scroll::ScrollableElement,
    table::{DataTable, TableState},
    text::{TextView, TextViewState},
    v_flex,
};
use std::path::PathBuf;

/// Reading width the Markdown preview is capped at when the width limit is enabled.
const MARKDOWN_PREVIEW_MAX_WIDTH: f32 = 800.0;

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

    /// Handle a right-click on the markdown preview to show its context menu.
    ///
    /// ### Arguments
    /// - `event`: The mouse-down event
    /// - `window`: The window context
    /// - `cx`: The application context
    fn on_preview_right_click(
        &mut self,
        event: &MouseDownEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if event.button != MouseButton::Right {
            return;
        }
        cx.stop_propagation();

        let position = event.position;
        let preview_focus = self.markdown_preview_focus.clone();

        // The replacement, `gpui_base::TextSelection::selected_text`, lives in the
        // `gpui-base` crate, which gpui-component does not re-export and which Fulgur
        // does not depend on directly. `WindowExt::selected_text` forwards to it.
        #[allow(deprecated)]
        let selection = window.selected_text(cx).trim().to_string();
        let has_selection = !selection.is_empty();
        self.markdown_preview_pending_copy = has_selection.then_some(selection);

        let menu = PopupMenu::build(window, cx, move |menu, _window, _cx| {
            menu.action_context(preview_focus)
                .menu_with_enable("Copy", Box::new(gpui_component::input::Copy), has_selection)
                .menu("Select All", Box::new(gpui_component::input::SelectAll))
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

    /// Resolve the text view state backing the currently visible markdown preview.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(Entity<TextViewState>)`: The state of the active preview, either a
    ///   dedicated preview tab or the inline panel of a markdown editor tab.
    /// - `None`: If no markdown preview is currently displayed.
    fn active_markdown_preview_state(&self, cx: &App) -> Option<Entity<TextViewState>> {
        match self.active_tab(cx)? {
            Tab::MarkdownPreview(preview) => Some(preview.view_state.clone()),
            Tab::Editor(_) => self.markdown_panel_view_state.clone(),
            Tab::Settings(_) => None,
        }
    }

    /// Ensure the inline preview panel owns a persistent text view state.
    ///
    /// ### Arguments
    /// - `text`: The rendered markdown source to display
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Entity<TextViewState>`: The persistent state for the inline preview.
    fn ensure_markdown_panel_state(
        &mut self,
        text: &str,
        cx: &mut Context<Self>,
    ) -> Entity<TextViewState> {
        let state = self
            .markdown_panel_view_state
            .get_or_insert_with(|| cx.new(|cx| TextViewState::markdown(text, cx)))
            .clone();
        state.update(cx, |state, cx| state.set_text(text, cx));
        state
    }

    /// Build the link activation handler for a markdown preview.
    ///
    /// ### Arguments
    /// - `base_dir`: The directory of the previewed file, used to resolve
    ///   relative links. `None` for a buffer that has never been saved.
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl Fn(...)`: A handler that hands remote links to the system and
    ///   opens local files in a tab of this window.
    fn markdown_link_handler(
        base_dir: Option<PathBuf>,
        cx: &Context<Self>,
    ) -> impl Fn(&SharedString, &ClickEvent, &mut Window, &mut App) + use<> {
        let view = cx.entity().downgrade();
        move |target, _event, window, cx| {
            match resolve_markdown_link(target, base_dir.as_deref()) {
                Some(MarkdownLinkTarget::External(url)) => {
                    log::debug!("Opening markdown preview link {url} in the system handler");
                    cx.open_url(&url);
                }
                Some(MarkdownLinkTarget::LocalFile(path)) => {
                    if !path.is_file() {
                        log::warn!(
                            "Markdown preview link points at a missing file: {}",
                            path.display()
                        );
                        window.push_notification(
                            (
                                NotificationType::Warning,
                                SharedString::from(format!(
                                    "Cannot open '{}': the file does not exist",
                                    path.display()
                                )),
                            ),
                            cx,
                        );
                        return;
                    }
                    view.update(cx, |this, cx| this.do_open_file(window, cx, path))
                        .ok();
                }
                // An anchor has no scroll target in the preview, and an
                // unresolvable reference has nowhere to go.
                Some(MarkdownLinkTarget::Anchor) | None => {}
            }
        }
    }

    /// Build the copy affordance shown on a markdown preview code block.
    ///
    /// ### Arguments
    /// - `code`: The code block contents to place on the clipboard
    ///
    /// ### Returns
    /// - `CopyButton`: The compact copy button for that code block
    fn markdown_code_block_copy(code: impl Into<SharedString>) -> CopyButton {
        CopyButton::new("copy-code-block").compact().value(code)
    }

    /// Lay out a markdown preview inside its container, honouring the width limit setting.
    ///
    /// ### Arguments
    /// - `preview`: The preview text view to lay out
    ///
    /// ### Returns
    /// - `Div`: A full-size container holding the preview, capped at
    ///   `MARKDOWN_PREVIEW_MAX_WIDTH` and centered when the limit is enabled.
    fn layout_markdown_preview(&self, preview: TextView) -> Div {
        let limited = self
            .settings
            .editor_settings
            .markdown_settings
            .limit_preview_width;
        h_flex()
            .size_full()
            .justify_center()
            .child(preview.when(limited, |view| view.max_w(px(MARKDOWN_PREVIEW_MAX_WIDTH))))
    }

    /// Wrap a markdown preview element with its context-menu affordances.
    ///
    /// ### Arguments
    /// - `child`: The preview element to wrap
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The wrapped, right-clickable preview element.
    fn wrap_markdown_preview(&self, child: AnyElement, cx: &mut Context<Self>) -> impl IntoElement {
        let right_click = cx.listener(|this, event: &MouseDownEvent, window, cx| {
            this.on_preview_right_click(event, window, cx);
        });
        let copy = cx.listener(|this, _: &gpui_component::input::Copy, _window, cx| {
            if let Some(text) = this.markdown_preview_pending_copy.take() {
                cx.write_to_clipboard(gpui::ClipboardItem::new_string(text));
            }
        });
        let select_all = cx.listener(|this, _: &gpui_component::input::SelectAll, _window, cx| {
            if let Some(state) = this.active_markdown_preview_state(cx) {
                state.update(cx, TextViewState::select_all);
            }
        });

        div()
            .id("markdown-preview-context")
            .track_focus(&self.markdown_preview_focus)
            .size_full()
            .capture_any_mouse_down(right_click)
            .on_action(copy)
            .on_action(select_all)
            .child(child)
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
                large_file: bool,
                content: Entity<EditorState>,
                path: Option<std::path::PathBuf>,
                csv_view_mode: editor_tab::CsvViewMode,
                csv_table: Option<Entity<TableState<editor_tab::CsvTableDelegate>>>,
                log_view: bool,
                log_content: Option<Entity<EditorState>>,
            },
            Settings,
            MarkdownPreview {
                content: Entity<EditorState>,
                source_path: Option<std::path::PathBuf>,
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
                    large_file: editor_tab.large_file,
                    content: editor_tab.content.clone(),
                    path: editor_tab.location.local_path().cloned(),
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
                    source_path: tabs_ref.iter().find_map(|t| match t.read(cx) {
                        Tab::Editor(editor_tab) if editor_tab.id == preview_tab.source_tab_id => {
                            editor_tab.location.local_path().cloned()
                        }
                        _ => None,
                    }),
                    view_state: preview_tab.view_state.clone(),
                },
            })
        });

        if let Some(tab) = active_tab {
            match tab {
                ActiveTabRenderData::Editor {
                    language,
                    show_markdown_preview,
                    large_file,
                    content,
                    path,
                    csv_view_mode,
                    csv_table,
                    log_view,
                    log_content,
                } => {
                    if log_view && let Some(log_content) = log_content {
                        let log_input = Editor::new(&log_content)
                            .disabled(true)
                            .bordered(false)
                            .p_0()
                            .h_full()
                            .font_family(self.settings.editor_settings.font_family.clone())
                            .text_size(px(self.settings.editor_settings.font_size));
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
                    let editor_input = Editor::new(&content)
                        .bordered(false)
                        .p_0()
                        .h_full()
                        .font_family(self.settings.editor_settings.font_family.clone())
                        .text_size(px(self.settings.editor_settings.font_size));
                    let capture_right_click =
                        cx.listener(|this, event: &MouseDownEvent, window, cx| {
                            this.on_editor_right_click(event, window, cx);
                        });
                    if language == SupportedLanguage::Markdown
                        && show_markdown_preview
                        && !large_file
                        && self.settings.editor_settings.markdown_settings.preview_mode
                            == crate::fulgur::settings::MarkdownPreviewMode::Panel
                    {
                        // Reading the content entity here tracks it for this
                        // window, so edits re-render the panel automatically.
                        // Multi-line raw-HTML blocks are collapsed first so the
                        // Markdown renderer never shapes a run containing a
                        // newline (see `sanitize_markdown_preview`).
                        let preview_text =
                            crate::fulgur::utils::sanitize::sanitize_markdown_preview(
                                &crate::fulgur::utils::markdown_images::rewrite_markdown_image_paths(
                                    content.read(cx).value().as_ref(),
                                    path.as_deref().and_then(std::path::Path::parent),
                                ),
                            );
                        let preview_state = self.ensure_markdown_panel_state(&preview_text, cx);
                        let link_handler = Self::markdown_link_handler(
                            path.as_deref()
                                .and_then(std::path::Path::parent)
                                .map(std::path::Path::to_path_buf),
                            cx,
                        );
                        let preview = self
                            .layout_markdown_preview(
                                TextView::new(&preview_state)
                                    .flex_none()
                                    .py_0()
                                    .px_2()
                                    .scrollable(true)
                                    .selectable(true)
                                    .on_link_click(link_handler)
                                    .code_block_actions(|code_block, _window, _cx| {
                                        Self::markdown_code_block_copy(code_block.code())
                                    }),
                            )
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
                ActiveTabRenderData::MarkdownPreview {
                    content,
                    source_path,
                    view_state,
                } => {
                    let base_dir = source_path.as_deref().and_then(std::path::Path::parent);
                    let preview_text = crate::fulgur::utils::sanitize::sanitize_markdown_preview(
                        &crate::fulgur::utils::markdown_images::rewrite_markdown_image_paths(
                            content.read(cx).value().as_ref(),
                            base_dir,
                        ),
                    );
                    view_state.update(cx, |state, cx| {
                        state.set_text(&preview_text, cx);
                    });
                    let link_handler =
                        Self::markdown_link_handler(base_dir.map(std::path::Path::to_path_buf), cx);
                    let preview = self
                        .layout_markdown_preview(
                            TextView::new(&view_state)
                                .py_2()
                                .px_4()
                                .scrollable(true)
                                .selectable(true)
                                .on_link_click(link_handler)
                                .code_block_actions(|code_block, _window, _cx| {
                                    Self::markdown_code_block_copy(code_block.code())
                                }),
                        )
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
