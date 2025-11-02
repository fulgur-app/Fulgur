use gpui::*;
use gpui_component::{
    IconName, Sizable, StyledExt,
    button::{Button, ButtonVariants},
};

// Create a button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button
// @param icon: The icon of the button
// @param border_color: The color of the border
// @return: The button
pub fn button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: IconName,
    border_color: Hsla,
) -> Button {
    Button::new(id)
        .icon(icon)
        .text()
        .small()
        .tooltip(tooltip)
        .ghost()
        .h(px(40.))
        .w(px(40.))
        .p_0()
        .m_0()
        .border_0()
        .border_color(border_color)
        .cursor_pointer()
        .corner_radii(Corners {
            top_left: px(0.0),
            top_right: px(0.0),
            bottom_left: px(0.0),
            bottom_right: px(0.0),
        })
}
