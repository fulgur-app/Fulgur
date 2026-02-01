use crate::fulgur::{
    Fulgur,
    ui::{
        components_utils::{EMPTY, UTF_8},
        icons::CustomIcon,
        languages::{self, SupportedLanguage},
    },
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{ActiveTheme, Icon, h_flex, input::Position};

/// Create a status bar item
///
/// ### Arguments
/// - `content`: The content of the status bar item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Div`: A status bar item
pub fn status_bar_item_factory(content: impl IntoElement, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

/// Create a status bar button
///
/// ### Arguments
/// - `content`: The content of the status bar button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
///
/// ### Returns
/// - `Div`: A status bar button
pub fn status_bar_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
) -> Div {
    status_bar_item_factory(content, border_color)
        .hover(|this| this.bg(accent_color))
        .cursor_pointer()
}

/// Create a status bar item, right hand side
///
/// ### Arguments
/// - `content`: The content of the status bar right item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `impl IntoElement`: A status bar right item
pub fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_l_1()
}

/// Create a status bar toggle button
///
/// ### Arguments
/// - `content`: The content of the status bar toggle button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
/// - `checked`: Whether the toggle is checked
///
/// ### Returns
/// - `Div`: A status bar toggle button
pub fn status_bar_toggle_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Div {
    let mut button = status_bar_button_factory(content, border_color, accent_color);
    if checked {
        button = button.bg(accent_color);
    }
    button
}

/// Create a status bar sync button
///
/// ### Arguments
/// - `connected_icon`: The icon to display when connected
/// - `disconnected_icon`: The icon to display when disconnected
/// - `border_color`: The color of the border
/// - `connected_color`: The color of the connected button
/// - `connected_foreground_color`: The foreground color of the connected button
/// - `connected_hover_color`: The hover color of the connected button
/// - `disconnected_color`: The color of the disconnected button
/// - `disconnected_foreground_color`: The foreground color of the disconnected button
/// - `disconnected_hover_color`: The hover color of the disconnected button
/// - `is_connected`: Whether the device is connected
///
/// ### Returns
/// - `Div`: A status bar sync button
pub fn status_bar_sync_button(
    connected_icon: Icon,
    disconnected_icon: Icon,
    border_color: Hsla,
    connected_color: Hsla,
    connected_foreground_color: Hsla,
    connected_hover_color: Hsla,
    disconnected_color: Hsla,
    disconnected_foreground_color: Hsla,
    disconnected_hover_color: Hsla,
    is_connected: bool,
) -> Div {
    let mut button = div()
        .text_sm()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .py_1()
        .border_color(border_color)
        .cursor_pointer();
    if is_connected {
        button = button
            .child(connected_icon)
            .bg(connected_color)
            .text_color(connected_foreground_color)
            .hover(|this| this.bg(connected_hover_color));
    } else {
        button = button
            .child(disconnected_icon)
            .bg(disconnected_color)
            .text_color(disconnected_foreground_color)
            .hover(|this| this.bg(disconnected_hover_color));
    }
    button
}

/// Create a status bar left item
///
/// ### Arguments
/// - `content`: The content of the status bar left item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `impl IntoElement`: A status bar left item
#[allow(dead_code)]
pub fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_r_1()
}

impl Fulgur {
    /// Render the status bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered status bar element
    pub fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (cursor_pos, language) = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    (
                        editor_tab.content.read(cx).cursor_position(),
                        Some(editor_tab.language),
                    )
                } else {
                    (Position::default(), Some(SupportedLanguage::Plain))
                }
            }
            None => (Position::default(), None),
        };
        let language = match language {
            Some(language) => languages::pretty_name(&language),
            None => EMPTY.to_string(),
        };
        let encoding = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.encoding.clone()
                } else {
                    EMPTY.to_string()
                }
            }
            None => UTF_8.to_string(),
        };
        let jump_to_line_button_content = format!(
            "Ln {}, Col {}",
            cursor_pos.line + 1,
            cursor_pos.character + 1
        );
        let jump_to_line_button = status_bar_button_factory(
            jump_to_line_button_content,
            cx.theme().border,
            cx.theme().muted,
        );
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
        let active_editor_tab = self.get_active_editor_tab();
        let show_markdown_preview = active_editor_tab.unwrap().show_markdown_preview; //TODO: Handle the case where there is no active editor tab even if it shouldn't happen
        let preview_button = status_bar_toggle_button_factory(
            "Preview".to_string(),
            cx.theme().border,
            cx.theme().muted,
            show_markdown_preview,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                let active_editor_tab = this.get_active_editor_tab_mut();
                if let Some(active_editor_tab) = active_editor_tab {
                    active_editor_tab.show_markdown_preview =
                        !active_editor_tab.show_markdown_preview;
                }
                cx.notify();
            }),
        );
        let show_markdown_toolbar = active_editor_tab.unwrap().show_markdown_toolbar; //TODO: Handle the case where there is no active editor tab even if it shouldn't happen
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
        let is_markdown = self.is_markdown();
        let is_connected = self.is_connected(cx);
        let sync_button = status_bar_sync_button(
            Icon::new(CustomIcon::Zap),
            Icon::new(CustomIcon::ZapOff),
            cx.theme().border,
            cx.theme().primary,
            cx.theme().primary_foreground,
            cx.theme().primary_hover,
            cx.theme().danger,
            cx.theme().danger_foreground,
            cx.theme().danger_hover,
            is_connected,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                this.open_share_file_sheet(window, cx);
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
                        self.settings
                            .app_settings
                            .synchronization_settings
                            .is_synchronization_activated,
                        |this| this.child(sync_button),
                    )
                    .child(language_button)
                    .when(is_markdown, |this| this.child(preview_button))
                    .when(is_markdown, |this| this.child(toolbar_button)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(jump_to_line_button)
                    .child(status_bar_right_item_factory(encoding, cx.theme().border)),
            )
    }
}
