use crate::fulgur::ui::icons::CustomIcon;
use gpui_kit::component::{
    Sizable, StyledExt,
    button::{Button, ButtonVariants},
};
use gpui_kit::{Corners, DefiniteLength, Hsla, Pixels, Styled, px, relative};
use std::time::SystemTime;
use time::{OffsetDateTime, UtcOffset};

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
pub const LINE_HEIGHT: DefiniteLength = relative(1.1);
/// The UTF-8 encoding
pub const UTF_8: &str = "UTF-8";
/// The untitled string
pub const UNTITLED: &str = "Untitled";
/// Maximum number of characters kept when renaming a tab
pub const MAX_TAB_NAME_LENGTH: usize = 100;
/// The empty string
pub const EMPTY: &str = "";

/// Create a button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button, also announced as its accessible name
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
        .accessibility_label(tooltip)
        .ghost()
        .p_0()
        .m_0()
        .border_0()
        .border_color(border_color)
        .cursor_pointer()
        .corner_radii(CORNERS_SIZE)
}

/// Returns the platform-appropriate label for the "reveal in file manager" action.
///
/// - **macOS**: "Reveal in Finder"
/// - **Windows**: "Reveal in Explorer"
/// - **Linux / other**: "Reveal in File Manager"
pub fn reveal_in_file_manager_label() -> &'static str {
    #[cfg(target_os = "macos")]
    {
        "Reveal in Finder"
    }
    #[cfg(target_os = "windows")]
    {
        "Reveal in Explorer"
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        "Reveal in File Manager"
    }
}

/// Format a system time as a date and time in the local time zone
///
/// ### Arguments
/// - `time`: The time to format
///
/// ### Returns
/// - `Some(String)`: The formatted date, as `YYYY-MM-DD HH:MM:SS`
/// - `None`: If the time could not be formatted
pub fn format_system_time(time: SystemTime) -> Option<String> {
    format_local_datetime(OffsetDateTime::from(time))
}

/// Format a date and time in the local time zone. Falls back to UTC
/// when the platform cannot report the local offset.
///
/// ### Arguments
/// - `datetime`: The date and time to format
///
/// ### Returns
/// - `Some(String)`: The formatted date, as `YYYY-MM-DD HH:MM:SS`
/// - `None`: If the date could not be formatted
pub fn format_local_datetime(datetime: OffsetDateTime) -> Option<String> {
    let offset = UtcOffset::local_offset_at(datetime).unwrap_or(UtcOffset::UTC);
    format_datetime_in(datetime, offset)
}

/// Format a date and time at a given UTC offset
///
/// ### Arguments
/// - `datetime`: The date and time to format
/// - `offset`: The UTC offset to display the date and time in
///
/// ### Returns
/// - `Some(String)`: The formatted date, as `YYYY-MM-DD HH:MM:SS`
/// - `None`: If the date could not be formatted
fn format_datetime_in(datetime: OffsetDateTime, offset: UtcOffset) -> Option<String> {
    let format = time::format_description::parse_borrowed::<2>(
        "[year]-[month]-[day] [hour]:[minute]:[second]",
    )
    .ok()?;
    datetime.to_offset(offset).format(&format).ok()
}

/// Format file size in a human-readable format.
///
/// ### Arguments
/// - `bytes`: File size in bytes
///
/// ### Returns
/// - `String`: Human-readable file size
// File sizes are cast to f64 only for display rounding; exactness beyond f64's
// 52-bit integer range (4 PB) is never needed here.
#[allow(clippy::cast_precision_loss)]
pub fn format_file_size(bytes: u64) -> String {
    if bytes < 1024 {
        format!("{bytes} B")
    } else if bytes < 1024 * 1024 {
        format!("{:.1} KB", bytes as f64 / 1024.0)
    } else if bytes < 1024 * 1024 * 1024 {
        format!("{:.1} MB", bytes as f64 / (1024.0 * 1024.0))
    } else {
        format!("{:.1} GB", bytes as f64 / (1024.0 * 1024.0 * 1024.0))
    }
}

#[cfg(test)]
mod tests {
    use core::prelude::v1::test;
    use gpui_kit::component::button::Button;
    use gpui_kit::{px, red, relative};
    use std::time::{Duration, UNIX_EPOCH};
    use time::{OffsetDateTime, UtcOffset};

    use crate::fulgur::ui::{
        components_utils::{
            CORNERS_SIZE, EMPTY, LINE_HEIGHT, MARKDOWN_BAR_BUTTON_SIZE, MARKDOWN_BAR_HEIGHT,
            SEARCH_BAR_BUTTON_SIZE, SEARCH_BAR_HEIGHT, TAB_BAR_BUTTON_SIZE, TAB_BAR_HEIGHT,
            TEXT_SIZE, UNTITLED, UTF_8, button_factory, format_datetime_in, format_file_size,
            format_system_time, reveal_in_file_manager_label,
        },
        icons::CustomIcon,
    };

    #[test]
    fn test_constants_are_consistent() {
        assert_eq!(TAB_BAR_HEIGHT, px(34.0));
        assert_eq!(TAB_BAR_BUTTON_SIZE, TAB_BAR_HEIGHT);
        assert_eq!(SEARCH_BAR_HEIGHT, px(40.0));
        assert_eq!(SEARCH_BAR_BUTTON_SIZE, SEARCH_BAR_HEIGHT);
        assert_eq!(MARKDOWN_BAR_HEIGHT, px(34.0));
        assert_eq!(MARKDOWN_BAR_BUTTON_SIZE, MARKDOWN_BAR_HEIGHT);
        assert_eq!(TEXT_SIZE, px(14.0));
        assert_eq!(LINE_HEIGHT, relative(1.1));
        assert_eq!(UTF_8, "UTF-8");
        assert_eq!(UNTITLED, "Untitled");
        assert_eq!(EMPTY, "");

        assert_eq!(CORNERS_SIZE.top_left, px(0.0));
        assert_eq!(CORNERS_SIZE.top_right, px(0.0));
        assert_eq!(CORNERS_SIZE.bottom_left, px(0.0));
        assert_eq!(CORNERS_SIZE.bottom_right, px(0.0));
    }

    #[test]
    fn test_button_factory_builds_button() {
        fn takes_button(_: Button) {}
        let button = button_factory("test-button", "Tooltip", CustomIcon::Search, red());
        takes_button(button);
    }

    #[test]
    fn test_reveal_in_file_manager_label_for_current_platform() {
        let label = reveal_in_file_manager_label();

        #[cfg(target_os = "macos")]
        assert_eq!(label, "Reveal in Finder");

        #[cfg(target_os = "windows")]
        assert_eq!(label, "Reveal in Explorer");

        #[cfg(not(any(target_os = "macos", target_os = "windows")))]
        assert_eq!(label, "Reveal in File Manager");
    }

    #[test]
    fn test_format_datetime_in_utc() {
        let datetime = OffsetDateTime::UNIX_EPOCH + time::Duration::seconds(3661);
        let formatted = format_datetime_in(datetime, UtcOffset::UTC);
        assert_eq!(formatted.as_deref(), Some("1970-01-01 01:01:01"));
    }

    #[test]
    fn test_format_datetime_in_shifts_to_the_given_offset() {
        let offset = UtcOffset::from_hms(-2, -30, 0).expect("valid offset");
        let formatted = format_datetime_in(OffsetDateTime::UNIX_EPOCH, offset);
        assert_eq!(formatted.as_deref(), Some("1969-12-31 21:30:00"));
    }

    #[test]
    fn test_format_system_time_uses_the_local_offset() {
        let ts = UNIX_EPOCH + Duration::from_secs(3661);
        let datetime = OffsetDateTime::from(ts);
        let offset = UtcOffset::local_offset_at(datetime).unwrap_or(UtcOffset::UTC);
        assert_eq!(format_system_time(ts), format_datetime_in(datetime, offset));
    }

    #[test]
    fn test_format_file_size_bytes_range() {
        assert_eq!(format_file_size(0), "0 B");
        assert_eq!(format_file_size(1), "1 B");
        assert_eq!(format_file_size(1023), "1023 B");
    }

    #[test]
    fn test_format_file_size_kilobytes_range() {
        assert_eq!(format_file_size(1024), "1.0 KB");
        assert_eq!(format_file_size(1536), "1.5 KB");
        assert_eq!(format_file_size(10 * 1024), "10.0 KB");
    }

    #[test]
    fn test_format_file_size_megabytes_range() {
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
        assert_eq!(format_file_size(1024 * 1024 + 512 * 1024), "1.5 MB");
        assert_eq!(format_file_size(10 * 1024 * 1024), "10.0 MB");
    }

    #[test]
    fn test_format_file_size_boundary_at_one_megabyte() {
        assert_eq!(format_file_size(1024 * 1024 - 1), "1024.0 KB");
        assert_eq!(format_file_size(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn test_format_file_size_boundary_at_one_gigabyte() {
        assert_eq!(format_file_size(1024 * 1024 * 1024 - 1), "1024.0 MB");
        assert_eq!(format_file_size(1024 * 1024 * 1024), "1.0 GB");
    }
}
