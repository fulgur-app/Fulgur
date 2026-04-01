mod actions;
mod rendering;

#[cfg(test)]
mod tests;

use crate::fulgur::ui::{
    components_utils::{SEARCH_BAR_BUTTON_SIZE, button_factory},
    icons::CustomIcon,
};
use gpui::*;
use gpui_component::button::Button;

#[derive(Debug, Clone)]
pub struct SearchMatch {
    pub start: usize,
    pub end: usize,
    pub line: usize,
    pub col: usize,
}

/// Create a search bar button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Button`: A search bar button
pub fn search_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    button_factory(id, tooltip, icon, border_color)
        .h(SEARCH_BAR_BUTTON_SIZE)
        .w(SEARCH_BAR_BUTTON_SIZE)
}

/// Create a search bar toggle button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
/// - `background_color`: The background color when inactive
/// - `accent_color`: The background color when active
/// - `checked`: Whether the toggle is checked
///
/// ### Returns
/// - `Button`: A search bar toggle button
pub fn search_bar_toggle_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
    background_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Button {
    let mut button = search_bar_button_factory(id, tooltip, icon, border_color);
    if checked {
        button = button.bg(accent_color);
    } else {
        button = button.bg(background_color);
    }
    button
}
