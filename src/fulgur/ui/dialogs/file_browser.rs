use gpui_kit::component::{
    Disableable, FocusableExt, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    input::{Input, InputEvent, InputGroup, InputGroupAddon, InputGroupAddonAlignment, InputState},
    scroll::ScrollableElement,
    v_flex,
};
use gpui_kit::{
    App, ClickEvent, Entity, FontWeight, Hsla, InteractiveElement, IntoElement, ParentElement,
    Pixels, SharedString, StatefulInteractiveElement, Styled, Window, div, px,
};

use crate::fulgur::ui::{
    icons::CustomIcon,
    menus::{GoToParentFolder, KEY_CONTEXT_PATH_BROWSER},
};

pub const MAX_VISIBLE_BROWSER_ROWS: usize = 10;
pub const BROWSER_ROW_HEIGHT_PX: f32 = 28.0;

/// Element id of the input group framing a browser path and its actions.
const BROWSER_PATH_FIELD_ID: &str = "browser-path-field";

/// Element id of the addon holding the browser path actions.
const BROWSER_PATH_ACTIONS_ID: &str = "browser-path-actions";

/// Element id of the button replacing a browser path with its parent folder.
pub const PARENT_FOLDER_BUTTON_ID: &str = "browser-parent-folder";

/// Label of the button replacing a browser path with its parent folder.
const PARENT_FOLDER_BUTTON_LABEL: &str = "Parent folder";

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

/// Truncate a browser path to its previous separator.
///
/// ### Description
/// Trailing separators are skipped first, so a folder path drops its last
/// folder while a partial name drops only that name.
///
/// ### Arguments
/// - `path`: Browser path, already trimmed.
/// - `is_separator`: Whether a character separates two path segments.
///
/// ### Returns
/// - `Some(&str)`: The path up to and including its previous separator.
/// - `None`: When the path has no segment left to drop.
pub fn truncate_to_parent_folder(path: &str, is_separator: fn(char) -> bool) -> Option<&str> {
    let folder = path.trim_end_matches(is_separator);
    let separator_index = folder.rfind(is_separator)?;
    Some(&path[..=separator_index])
}

/// Replace a browser input with its parent folder, when it has one.
///
/// ### Description
/// The replacement is undoable and emits `InputEvent::Change`, so the browser
/// lists the parent folder through its usual refresh.
///
/// ### Arguments
/// - `input`: Input state holding the browser path.
/// - `parent_folder`: Browser-specific parent of a raw input value.
/// - `window`: Window hosting the browser.
/// - `cx`: Application context.
pub fn replace_with_parent_folder(
    input: &Entity<InputState>,
    parent_folder: fn(&str) -> Option<String>,
    window: &mut Window,
    cx: &mut App,
) {
    let Some(parent) = parent_folder(&input.read(cx).value()) else {
        return;
    };
    input.update(cx, |state, cx| state.replace_all(parent, window, cx));
}

/// Render a browser path input with its parent folder button on the right.
///
/// ### Arguments
/// - `input`: Input state holding the browser path.
/// - `has_parent_folder`: Whether the path has a parent folder to go to.
/// - `on_parent_folder`: Handler run when the parent folder button is clicked.
///
/// ### Returns
/// - `InputGroup`: The input framed together with its parent folder button.
pub fn render_browser_path_field(
    input: &Entity<InputState>,
    has_parent_folder: bool,
    on_parent_folder: impl Fn(&ClickEvent, &mut Window, &mut App) + 'static,
) -> InputGroup {
    InputGroup::new(BROWSER_PATH_FIELD_ID)
        .focus_ring(false)
        .input(Input::new(input))
        .addon(
            InputGroupAddon::new(BROWSER_PATH_ACTIONS_ID)
                .align(InputGroupAddonAlignment::InlineEnd)
                .child(
                    Button::new(PARENT_FOLDER_BUTTON_ID)
                        .debug_selector(|| PARENT_FOLDER_BUTTON_ID.to_string())
                        .icon(CustomIcon::CornerLeftUp)
                        .ghost()
                        .xsmall()
                        .disabled(!has_parent_folder)
                        .tooltip_with_action(
                            PARENT_FOLDER_BUTTON_LABEL,
                            &GoToParentFolder,
                            Some(KEY_CONTEXT_PATH_BROWSER),
                        )
                        .accessibility_label(PARENT_FOLDER_BUTTON_LABEL)
                        .on_click(on_parent_folder),
                ),
        )
}

/// Compute the fixed pixel height for a browser file list.
///
/// ### Returns
/// - `Pixels`: Height fixed to `MAX_VISIBLE_BROWSER_ROWS` rows, forcing scrollbar
///   overflow beyond that.
// A small row-count constant cast to f32 for a pixel height; far within f32's
// exact-integer range.
#[allow(clippy::cast_precision_loss)]
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
    input_entity: &Entity<InputState>,
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

#[cfg(test)]
mod tests {
    use super::truncate_to_parent_folder;
    use core::prelude::v1::test;

    /// Separator predicate of the remote browser, used to keep the tests platform-independent.
    fn is_slash(c: char) -> bool {
        c == '/'
    }

    #[test]
    fn test_truncate_to_parent_folder_drops_the_last_folder_of_a_folder_path() {
        assert_eq!(
            truncate_to_parent_folder("/var/log/", is_slash),
            Some("/var/")
        );
        assert_eq!(
            truncate_to_parent_folder("/var/log", is_slash),
            Some("/var/")
        );
    }

    #[test]
    fn test_truncate_to_parent_folder_drops_only_a_partial_name() {
        assert_eq!(
            truncate_to_parent_folder("/var/log/sys", is_slash),
            Some("/var/log/")
        );
    }

    #[test]
    fn test_truncate_to_parent_folder_reaches_the_root() {
        assert_eq!(truncate_to_parent_folder("/var/", is_slash), Some("/"));
    }

    #[test]
    fn test_truncate_to_parent_folder_has_nothing_to_drop_at_the_root() {
        assert_eq!(truncate_to_parent_folder("/", is_slash), None);
        assert_eq!(truncate_to_parent_folder("//", is_slash), None);
        assert_eq!(truncate_to_parent_folder("", is_slash), None);
    }

    #[test]
    fn test_truncate_to_parent_folder_needs_a_separator() {
        assert_eq!(truncate_to_parent_folder("notes.txt", is_slash), None);
    }

    #[test]
    fn test_truncate_to_parent_folder_uses_the_given_separators() {
        let is_any_separator = |c: char| c == '/' || c == '\\';
        assert_eq!(
            truncate_to_parent_folder(r"C:\Users\me\", is_any_separator),
            Some(r"C:\Users\")
        );
    }
}
