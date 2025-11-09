use gpui::*;
use gpui_component::{
    Sizable, StyledExt,
    button::{Button, ButtonVariants},
};

use crate::fulgur::icons::CustomIcon;

// The height of the tab bar
pub const TAB_BAR_HEIGHT: Pixels = px(34.0);
// The height of the search bar
pub const SEARCH_BAR_HEIGHT: Pixels = px(40.0);
// The size of the button
pub const BUTTON_SIZE: Pixels = TAB_BAR_HEIGHT;
// The size of the corners of the button
pub const CORNERS_SIZE: Corners<Pixels> = Corners {
    top_left: px(0.0),
    top_right: px(0.0),
    bottom_left: px(0.0),
    bottom_right: px(0.0),
};
// The size of the text
pub const TEXT_SIZE: Pixels = px(14.0);
// The line height of the text inputs
pub const LINE_HEIGHT: DefiniteLength = relative(1.0);
// The UTF-8 encoding
pub const UTF_8: &str = "UTF-8";
// The untitled string
pub const UNTITLED: &str = "Untitled";

// Create a button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button for the button
// @param icon: The icon of the button
// @param border_color: The color of the border of the button
// @return: The button
pub fn button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    Button::new(id)
        .icon(icon)
        .text()
        .small()
        .tooltip(tooltip)
        .ghost()
        .h(BUTTON_SIZE)
        .w(BUTTON_SIZE)
        .p_0()
        .m_0()
        .border_0()
        .border_color(border_color)
        .cursor_pointer()
        .corner_radii(CORNERS_SIZE)
}
