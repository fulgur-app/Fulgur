use std::time::SystemTime;
use time::OffsetDateTime;

use gpui::*;
use gpui_component::{
    IndexPath, Sizable, StyledExt,
    button::{Button, ButtonVariants},
    select::SelectState,
};

use crate::fulgur::ui::icons::CustomIcon;

/// The height of the tab bar
pub const TAB_BAR_HEIGHT: Pixels = px(34.0);
/// The size of the tab barbutton
pub const TAB_BAR_BUTTON_SIZE: Pixels = TAB_BAR_HEIGHT;
/// The height of the search bar
pub const SEARCH_BAR_HEIGHT: Pixels = px(40.0);
/// The size of the search bar button
pub const SEARCH_BAR_BUTTON_SIZE: Pixels = SEARCH_BAR_HEIGHT;
/// The height of the markdown bar
pub const MARKDOWN_BAR_HEIGHT: Pixels = px(34.0);
/// The size of the markdown bar button
pub const MARKDOWN_BAR_BUTTON_SIZE: Pixels = MARKDOWN_BAR_HEIGHT;
/// The size of the corners of the button
pub const CORNERS_SIZE: Corners<Pixels> = Corners {
    top_left: px(0.0),
    top_right: px(0.0),
    bottom_left: px(0.0),
    bottom_right: px(0.0),
};
/// The size of the text
pub const TEXT_SIZE: Pixels = px(14.0);
/// The line height of the text inputs
pub const LINE_HEIGHT: DefiniteLength = relative(1.0);
/// The UTF-8 encoding
pub const UTF_8: &str = "UTF-8";
/// The untitled string
pub const UNTITLED: &str = "Untitled";
/// The empty string
pub const EMPTY: &str = "";

/// Create a button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button for the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border of the button
///
/// ### Returns
/// - `Button`: The button
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
        .p_0()
        .m_0()
        .border_0()
        .border_color(border_color)
        .cursor_pointer()
        .corner_radii(CORNERS_SIZE)
}

/// Create the a select state
///
/// ### Arguments
/// - `window`: The window
/// - `current_value`: The current value
/// - `options`: The options
/// - `cx`: The application context
///
/// ### Returns
/// - `Entity<SelectState<Vec<SharedString>>>`: The select state entity
pub fn create_select_state(
    window: &mut Window,
    current_value: String,
    options: Vec<SharedString>,
    cx: &mut App,
) -> Entity<SelectState<Vec<SharedString>>> {
    let selected_index = options.iter().position(|s| s.as_ref() == current_value);
    cx.new(|cx| {
        SelectState::new(
            options,
            selected_index.map(|i| IndexPath::default().row(i)),
            window,
            cx,
        )
    })
}

/// Format a date as ISO 8601 string
///
/// ### Arguments
/// - `time`: The time to format
///
/// ### Returns
/// - `Some(String)`: The formatted date
/// - `None`: If the time could not be formatted
pub fn format_system_time(time: SystemTime) -> Option<String> {
    let datetime = OffsetDateTime::from(time);
    let format =
        time::format_description::parse("[year]-[month]-[day] [hour]:[minute]:[second]").ok()?;
    datetime.format(&format).ok()
}
