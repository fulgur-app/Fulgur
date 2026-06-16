use crate::fulgur::ui::icons::CustomIcon;
use gpui::{
    Animation, AnimationExt, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Styled, div,
};
use gpui_component::Icon;
use std::f32::consts::PI;
use std::time::Duration;

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

/// Parameters for the sync button styling
pub struct SyncButtonStyle {
    pub connected_icon: Icon,
    pub disconnected_icon: Icon,
    pub border_color: Hsla,
    pub connected_color: Hsla,
    pub connected_foreground_color: Hsla,
    pub connected_hover_color: Hsla,
    pub disconnected_color: Hsla,
    pub disconnected_foreground_color: Hsla,
    pub disconnected_hover_color: Hsla,
    pub connecting_color: Hsla,
    pub connecting_foreground_color: Hsla,
}

/// The visual state of the sync button
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SyncButtonState {
    Connected,
    Connecting,
    Disconnected,
}

/// Create a status bar sync button
///
/// ### Arguments
/// - `style`: The styling parameters for the sync button
/// - `state`: The current sync button state
/// - `show_spinner`: Whether to show the spinning animation (only after delay)
///
/// ### Returns
/// - `Div`: A status bar sync button
pub fn status_bar_sync_button(
    style: SyncButtonStyle,
    state: SyncButtonState,
    show_spinner: bool,
) -> Div {
    let mut button = div()
        .text_sm()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .py_1()
        .border_color(style.border_color);
    match state {
        SyncButtonState::Connected => {
            button = button
                .child(style.connected_icon)
                .bg(style.connected_color)
                .text_color(style.connected_foreground_color)
                .hover(|this| this.bg(style.connected_hover_color))
                .cursor_pointer();
        }
        SyncButtonState::Connecting => {
            if show_spinner {
                let spinning_icon = Icon::new(CustomIcon::Zap).with_animation(
                    "sync-spinner",
                    Animation::new(Duration::from_secs(1)).repeat(),
                    |icon, delta| icon.rotate(gpui::radians(delta * 2.0 * PI)),
                );
                button = button
                    .child(spinning_icon)
                    .bg(style.connecting_color)
                    .text_color(style.connecting_foreground_color);
            } else {
                button = button
                    .child(style.connected_icon)
                    .bg(style.connecting_color)
                    .text_color(style.connecting_foreground_color);
            }
        }
        SyncButtonState::Disconnected => {
            button = button
                .child(style.disconnected_icon)
                .bg(style.disconnected_color)
                .text_color(style.disconnected_foreground_color)
                .hover(|this| this.bg(style.disconnected_hover_color))
                .cursor_pointer();
        }
    }
    button
}
