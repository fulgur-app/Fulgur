use super::widgets::{
    SyncButtonStyle, status_bar_button_factory, status_bar_right_item_factory,
    status_bar_sync_button, status_bar_toggle_button_factory,
};
use crate::fulgur::{
    Fulgur,
    languages::supported_languages::SupportedLanguage,
    settings::MarkdownPreviewMode,
    tab::Tab,
    ui::{icons::CustomIcon, tabs::editor_tab::CsvViewMode},
};
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
};
use gpui_component::{ActiveTheme, Icon, StyledExt, h_flex, tooltip::Tooltip, v_flex};

impl Fulgur {
    /// Render the status bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered status bar element
    pub fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let line_col = self.status_bar_cache.line_col.clone();
        let language = self.status_bar_cache.language_label.clone();
        let encoding = self.status_bar_cache.encoding_label.clone();
        let jump_to_line_button =
            status_bar_button_factory(line_col, cx.theme().border, cx.theme().muted);
        let jump_to_line_button = jump_to_line_button.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                this.show_jump_to_line_dialog(window, cx);
            }),
        );
        let language_button =
            status_bar_button_factory(language, cx.theme().border, cx.theme().muted).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    //set_language(this, window, cx, language_shared.clone());
                    this.render_select_language_sheet(window, cx);
                }),
            );
        let (preview_button, toolbar_button) = match self.get_active_editor_tab() {
            None => (div(), div()),
            Some(active_editor_tab) => {
                let editor_id = active_editor_tab.id;
                let preview_active = match self
                    .settings
                    .editor_settings
                    .markdown_settings
                    .preview_mode
                {
                    MarkdownPreviewMode::DedicatedTab => self.tabs.iter().any(
                        |t| matches!(t, Tab::MarkdownPreview(p) if p.source_tab_id == editor_id),
                    ),
                    MarkdownPreviewMode::Panel => active_editor_tab.show_markdown_preview,
                };
                let preview_button = status_bar_toggle_button_factory(
                    "Preview".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    preview_active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                        if this.settings.editor_settings.markdown_settings.preview_mode
                            == MarkdownPreviewMode::DedicatedTab
                        {
                            this.open_markdown_preview_tab(window, cx);
                        } else {
                            if let Some(active_editor_tab) = this.get_active_editor_tab_mut() {
                                active_editor_tab.show_markdown_preview =
                                    !active_editor_tab.show_markdown_preview;
                            }
                            cx.notify();
                        }
                    }),
                );
                let show_markdown_toolbar = active_editor_tab.show_markdown_toolbar;
                let toolbar_button = status_bar_toggle_button_factory(
                    "Toolbar".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    show_markdown_toolbar,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                        let active_editor_tab = this.get_active_editor_tab_mut();
                        if let Some(active_editor_tab) = active_editor_tab {
                            active_editor_tab.show_markdown_toolbar =
                                !active_editor_tab.show_markdown_toolbar;
                        }
                        cx.notify();
                    }),
                );
                (preview_button, toolbar_button)
            }
        };
        let is_csv = self.get_current_language() == SupportedLanguage::Csv;
        let csv_table_active = self
            .get_active_editor_tab()
            .is_some_and(|tab| tab.csv_view_mode == CsvViewMode::Table);
        let csv_view_button = status_bar_toggle_button_factory(
            "Table".to_string(),
            cx.theme().border,
            cx.theme().muted,
            csv_table_active,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                this.toggle_csv_view_mode(window, cx);
            }),
        );
        let is_markdown = self.is_markdown();
        let (sync_button_state, show_spinner) = self.status_bar_sync_button_state(cx);
        let profile_statuses = self.sync_profiles_tooltip_data(cx);
        let sync_button = status_bar_sync_button(
            SyncButtonStyle {
                connected_icon: Icon::new(CustomIcon::Zap),
                disconnected_icon: Icon::new(CustomIcon::ZapOff),
                border_color: cx.theme().border,
                connected_color: cx.theme().primary,
                connected_foreground_color: cx.theme().primary_foreground,
                connected_hover_color: cx.theme().primary_hover,
                disconnected_color: cx.theme().danger,
                disconnected_foreground_color: cx.theme().danger_foreground,
                disconnected_hover_color: cx.theme().danger_hover,
                connecting_color: cx.theme().warning,
                connecting_foreground_color: cx.theme().warning_foreground,
            },
            sync_button_state,
            show_spinner,
        )
        .id("sync-status-button")
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.open_share_file_sheet(window, cx);
            }),
        )
        .when(!profile_statuses.is_empty(), move |this| {
            this.tooltip(move |window, cx| {
                let rows = profile_statuses.clone();
                Tooltip::element(move |_, cx| {
                    let mut container = v_flex().gap_1().py_1().px_1();
                    for (name, label) in &rows {
                        container = container.child(
                            h_flex()
                                .gap_2()
                                .child(div().text_sm().font_semibold().child(format!("{name}:")))
                                .child(
                                    div()
                                        .text_sm()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(label.clone()),
                                ),
                        );
                    }
                    container
                })
                .build(window, cx)
            })
        });
        h_flex()
            .justify_between()
            .bg(cx.theme().tab_bar)
            .py_0()
            .my_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .justify_start()
                    .when(
                        self.settings
                            .app_settings
                            .synchronization_settings
                            .is_synchronization_activated,
                        |this| this.child(sync_button),
                    )
                    .child(language_button)
                    .when(is_markdown, |this| this.child(preview_button))
                    .when(is_markdown, |this| this.child(toolbar_button))
                    .when(is_csv, |this| this.child(csv_view_button)),
            )
            .child({
                let color_picker_active = self.color_picker_bar_state.show_color_picker;
                let color_button = status_bar_toggle_button_factory(
                    "Color".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    color_picker_active,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _event: &MouseDownEvent, _window, cx| {
                        this.color_picker_bar_state.show_color_picker =
                            !this.color_picker_bar_state.show_color_picker;
                        cx.notify();
                    }),
                );
                div()
                    .flex()
                    .justify_end()
                    .child(color_button)
                    .child(jump_to_line_button)
                    .child(status_bar_right_item_factory(encoding, cx.theme().border))
            })
    }
}
