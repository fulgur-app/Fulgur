mod actions;
mod rendering;

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

use gpui::{Action, Hsla, Styled};
use gpui_component::button::Button;
use serde::Deserialize;

use crate::fulgur::ui::{
    components_utils::{TAB_BAR_BUTTON_SIZE, button_factory},
    icons::CustomIcon,
};

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabAction(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabsToLeft(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabsToRight(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseAllOtherTabs(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct ShowInFileManager(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct DuplicateTab(pub usize);

gpui::actions!(fulgur, [CloseAllTabsAction, SendTabToWindowNoOp]);

/// Create a tab bar button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Button`: A tab bar button
pub fn tab_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    button_factory(id, tooltip, icon, border_color)
        .border_b_1()
        .h(TAB_BAR_BUTTON_SIZE)
        .w(TAB_BAR_BUTTON_SIZE)
}

/// Opens the theme repository in the default browser
///
/// This is a standalone helper function for the GetTheme action.
pub fn open_theme_repository() {
    if let Err(e) = open::that("https://github.com/longbridge/gpui-component/tree/main/themes") {
        log::error!("Failed to open browser: {}", e);
    }
}
