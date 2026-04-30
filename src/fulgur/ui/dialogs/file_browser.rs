use gpui::{
    Entity, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement, Pixels, SharedString,
    StatefulInteractiveElement, Styled, div, px,
};
use gpui_component::{
    h_flex,
    input::{InputEvent, InputState},
    scroll::ScrollableElement,
    v_flex,
};

use crate::fulgur::ui::icons::CustomIcon;

pub const MAX_VISIBLE_BROWSER_ROWS: usize = 10;
pub const BROWSER_ROW_HEIGHT_PX: f32 = 28.0;

/// A normalized browser entry used by `render_browser_list`.
pub struct BrowserEntry {
    pub id: SharedString,
    pub is_dir: bool,
    pub display_name: String,
    pub click_value: String,
}

/// Build a `BrowserEntry` from normalized path components.
///
/// ### Arguments
/// - `id`: Unique element ID for the row.
/// - `is_dir`: Whether this entry is a directory.
/// - `name`: Entry name without parent path.
/// - `full_path`: Full path as a string slice.
/// - `separator`: Character appended to directory names and click values.
///
/// ### Returns
/// - `BrowserEntry`: Normalized browser entry ready for `render_browser_list`.
pub fn build_browser_entry(
    id: SharedString,
    is_dir: bool,
    name: &str,
    full_path: &str,
    separator: char,
) -> BrowserEntry {
    BrowserEntry {
        id,
        is_dir,
        display_name: if is_dir {
            format!("{name}{separator}")
        } else {
            name.to_owned()
        },
        click_value: if is_dir {
            format!("{full_path}{separator}")
        } else {
            full_path.to_owned()
        },
    }
}

/// Compute the fixed pixel height for a browser file list.
///
/// ### Returns
/// - `Pixels`: Height fixed to `MAX_VISIBLE_BROWSER_ROWS` rows, forcing scrollbar
///   overflow beyond that.
pub fn browser_list_height() -> Pixels {
    px(MAX_VISIBLE_BROWSER_ROWS as f32 * BROWSER_ROW_HEIGHT_PX)
}

/// Render a scrollable file list from an iterator of browser entries.
///
/// ### Arguments
/// - `entries`: Iterator of `BrowserEntry` items to render.
/// - `input_entity`: Input state entity updated when a row is clicked.
/// - `hover_bg`: Background color applied on row hover.
/// - `icon_color`: Icon foreground color.
///
/// ### Returns
/// - `Some(impl IntoElement)`: A scrollable list element.
/// - `None`: When the iterator is empty, so callers can skip appending.
pub fn render_browser_list(
    entries: impl IntoIterator<Item = BrowserEntry>,
    input_entity: Entity<InputState>,
    hover_bg: Hsla,
    icon_color: Hsla,
) -> Option<impl IntoElement> {
    let list_height = browser_list_height();
    let mut list = v_flex().overflow_y_scrollbar().h(list_height).w_full();
    let mut has_entries = false;
    for entry in entries {
        has_entries = true;
        list = list.child(render_browser_row(
            entry.id,
            entry.is_dir,
            entry.display_name,
            entry.click_value,
            input_entity.clone(),
            hover_bg,
            icon_color,
        ));
    }
    has_entries.then_some(list)
}

/// Build a single browser entry row element.
///
/// ### Arguments
/// - `id`: Unique element ID for the row.
/// - `is_dir`: Whether this entry is a directory.
/// - `display_name`: Formatted entry name (with trailing separator for directories).
/// - `click_value`: Path string written into the path input when the row is clicked.
/// - `input_entity`: Input state entity updated on click.
/// - `hover_bg`: Background color applied on hover.
/// - `icon_color`: Icon foreground color.
///
/// ### Returns
/// - `impl IntoElement`: A rendered browser entry row.
pub fn render_browser_row(
    id: SharedString,
    is_dir: bool,
    display_name: String,
    click_value: String,
    input_entity: Entity<InputState>,
    hover_bg: Hsla,
    icon_color: Hsla,
) -> impl IntoElement {
    let icon = if is_dir {
        CustomIcon::FolderOpen
    } else {
        CustomIcon::File
    };
    let font_weight = if is_dir {
        FontWeight::SEMIBOLD
    } else {
        FontWeight::NORMAL
    };

    h_flex()
        .id(id)
        .w_full()
        .px_2()
        .py_1()
        .gap_2()
        .items_center()
        .cursor_pointer()
        .hover(move |h| h.bg(hover_bg))
        .child(icon.icon().size(px(14.)).text_color(icon_color))
        .child(div().text_sm().font_weight(font_weight).child(display_name))
        .on_click(move |_, window, cx| {
            input_entity.update(cx, |state, cx| {
                state.set_value(&click_value, window, cx);
                cx.emit(InputEvent::Change);
            });
        })
}
