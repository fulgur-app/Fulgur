use crate::fulgur::ui::icons::CustomIcon;
use gpui::{
    Animation, AnimationExt, Div, Hsla, InteractiveElement, IntoElement, ParentElement, Role,
    Stateful, StatefulInteractiveElement, Styled, accesskit::Toggled, div,
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
/// - `id`: The element ID, also the accessibility identifier of the button
/// - `content`: The content of the status bar button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
///
/// ### Returns
/// - `Stateful<Div>`: A status bar button
pub fn status_bar_button_factory(
    id: &'static str,
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
) -> Stateful<Div> {
    status_bar_item_factory(content, border_color)
        .id(id)
        .role(Role::Button)
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
/// - `id`: The element ID, also the accessibility identifier of the button
/// - `content`: The content of the status bar toggle button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
/// - `checked`: Whether the toggle is checked
///
/// ### Returns
/// - `Stateful<Div>`: A status bar toggle button
pub fn status_bar_toggle_button_factory(
    id: &'static str,
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Stateful<Div> {
    let mut button = status_bar_button_factory(id, content, border_color, accent_color)
        .aria_toggled(Toggled::from(checked));
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

impl SyncButtonState {
    /// The name a screen reader announces for the sync button in this state
    ///
    /// ### Returns
    /// - `&'static str`: The accessible name of the sync button
    pub fn accessibility_label(self) -> &'static str {
        match self {
            Self::Connected => "Synchronization connected, open sharing",
            Self::Connecting => "Synchronization connecting, open sharing",
            Self::Disconnected => "Synchronization disconnected, open sharing",
        }
    }
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

#[cfg(test)]
mod tests {
    use super::SyncButtonState;

    #[test]
    fn every_sync_state_has_a_distinct_accessible_name() {
        let labels = [
            SyncButtonState::Connected.accessibility_label(),
            SyncButtonState::Connecting.accessibility_label(),
            SyncButtonState::Disconnected.accessibility_label(),
        ];
        assert!(labels.iter().all(|label| !label.is_empty()));
        assert_ne!(labels[0], labels[1]);
        assert_ne!(labels[1], labels[2]);
        assert_ne!(labels[0], labels[2]);
    }
}
