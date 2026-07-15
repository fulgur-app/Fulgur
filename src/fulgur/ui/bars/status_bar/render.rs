use super::{
    state::{StatusBar, StatusBarEvent},
    widgets::{
        SyncButtonStyle, status_bar_button_factory, status_bar_right_item_factory,
        status_bar_sync_button, status_bar_toggle_button_factory,
    },
};
use crate::fulgur::{
    languages::supported_languages::SupportedLanguage,
    settings::MarkdownPreviewMode,
    tab::Tab,
    ui::{icons::CustomIcon, log_view::log_toggle_available, tabs::editor_tab::CsvViewMode},
};
use gpui::{
    Context, InteractiveElement, IntoElement, MouseButton, MouseDownEvent, ParentElement, Render,
    StatefulInteractiveElement, Styled, Window, div, prelude::FluentBuilder,
};
use gpui_component::{ActiveTheme, Icon, StyledExt, h_flex, tooltip::Tooltip, v_flex};

impl Render for StatusBar {
    /// Render the status bar from the owning window's current state
    ///
    /// ### Arguments
    /// - `_window`: The window to render the status bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered status bar element
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let Some(fulgur_entity) = self.fulgur.upgrade() else {
            return div().into_any_element();
        };
        let fulgur = fulgur_entity.read(cx);
        let labels = StatusBar::compute_labels(fulgur, cx);

        let jump_to_line_button =
            status_bar_button_factory(labels.line_col, cx.theme().border, cx.theme().muted)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                        cx.emit(StatusBarEvent::JumpToLine);
                    }),
                );
        let language_button =
            status_bar_button_factory(labels.language_label, cx.theme().border, cx.theme().muted)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                        cx.emit(StatusBarEvent::SelectLanguage);
                    }),
                );
        let (preview_button, toolbar_button) = match fulgur.get_active_editor_tab(cx) {
            None => (div(), div()),
            Some(active_editor_tab) => {
                let editor_id = active_editor_tab.id;
                let preview_active = match fulgur
                    .settings
                    .editor_settings
                    .markdown_settings
                    .preview_mode
                {
                    MarkdownPreviewMode::DedicatedTab => fulgur.tabs.iter().any(|t| {
                        matches!(t.read(cx), Tab::MarkdownPreview(p) if p.source_tab_id == editor_id)
                    }),
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
                    cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                        cx.emit(StatusBarEvent::ToggleMarkdownPreview);
                    }),
                );
                let toolbar_button = status_bar_toggle_button_factory(
                    "Toolbar".to_string(),
                    cx.theme().border,
                    cx.theme().muted,
                    active_editor_tab.show_markdown_toolbar,
                )
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                        cx.emit(StatusBarEvent::ToggleMarkdownToolbar);
                    }),
                );
                (preview_button, toolbar_button)
            }
        };
        let is_large_file = fulgur
            .get_active_editor_tab(cx)
            .is_some_and(|tab| tab.large_file);
        let is_csv = fulgur.get_current_language(cx) == SupportedLanguage::Csv;
        let csv_table_active = fulgur
            .get_active_editor_tab(cx)
            .is_some_and(|tab| tab.csv_view_mode == CsvViewMode::Table);
        let csv_view_button = status_bar_toggle_button_factory(
            "Table".to_string(),
            cx.theme().border,
            cx.theme().muted,
            csv_table_active,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                cx.emit(StatusBarEvent::ToggleCsvView);
            }),
        );
        let log_path = fulgur
            .get_active_editor_tab(cx)
            .and_then(|tab| tab.file_path().cloned());
        let log_toggle_visible = log_path.as_deref().is_some_and(log_toggle_available);
        let log_view_active = fulgur
            .get_active_editor_tab(cx)
            .is_some_and(|tab| tab.log_view);
        let log_follow_active = fulgur
            .get_active_editor_tab(cx)
            .is_some_and(|tab| tab.log_follow);
        let log_dropped = fulgur
            .get_active_editor_tab(cx)
            .map(|tab| tab.id)
            .and_then(|id| fulgur.log_tail_state.get(&id))
            .is_some_and(|state| state.dropped_lines);
        let log_button = status_bar_toggle_button_factory(
            "Log".to_string(),
            cx.theme().border,
            cx.theme().muted,
            log_view_active,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                cx.emit(StatusBarEvent::ToggleLogView);
            }),
        );
        let log_follow_button = status_bar_toggle_button_factory(
            "Follow".to_string(),
            cx.theme().border,
            cx.theme().muted,
            log_follow_active,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                cx.emit(StatusBarEvent::ToggleLogFollow);
            }),
        );
        let log_load_full_button =
            status_bar_button_factory("Load full".to_string(), cx.theme().border, cx.theme().muted)
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                        cx.emit(StatusBarEvent::LoadFullLog);
                    }),
                );
        let is_markdown = fulgur.is_markdown(cx);
        let profiles = &fulgur
            .settings
            .app_settings
            .synchronization_settings
            .profiles;
        let (sync_button_state, show_spinner) = StatusBar::sync_button_state(profiles, cx);
        let profile_statuses = StatusBar::sync_profiles_tooltip_data(profiles, cx);
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
            cx.listener(|_, _event, _window, cx| {
                cx.emit(StatusBarEvent::OpenShareSheet);
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
        let color_picker_active = fulgur.color_picker_bar.read(cx).is_visible();
        let color_button = status_bar_toggle_button_factory(
            "Color".to_string(),
            cx.theme().border,
            cx.theme().muted,
            color_picker_active,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|_, _event: &MouseDownEvent, _window, cx| {
                cx.emit(StatusBarEvent::ToggleColorPicker);
            }),
        );
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
                        fulgur
                            .settings
                            .app_settings
                            .synchronization_settings
                            .is_synchronization_activated,
                        |this| this.child(sync_button),
                    )
                    .when(!is_large_file, |this| this.child(language_button))
                    .when(is_markdown, |this| this.child(preview_button))
                    .when(is_markdown, |this| this.child(toolbar_button))
                    .when(is_csv, |this| this.child(csv_view_button))
                    .when(log_toggle_visible, |this| this.child(log_button))
                    .when(log_view_active, |this| this.child(log_follow_button))
                    .when(log_view_active && log_dropped, |this| {
                        this.child(log_load_full_button)
                    }),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(color_button)
                    .child(jump_to_line_button)
                    .child(status_bar_right_item_factory(
                        labels.encoding_label,
                        cx.theme().border,
                    )),
            )
            .into_any_element()
    }
}
