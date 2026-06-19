//! Read-only display-buffer helpers for the log view.

use gpui::{Context, Entity, Window};
use gpui_component::input::{InputState, Position};

use crate::fulgur::Fulgur;

/// Return the zero-based index of the last line in a buffer.
///
/// ### Arguments
/// - `text`: The buffer text
///
/// ### Returns
/// - `u32`: The last line index, clamped to `u32::MAX`
fn last_line_index(text: &str) -> u32 {
    u32::try_from(text.matches('\n').count()).unwrap_or(u32::MAX)
}

/// Replace a log buffer's content and snap the view to the last line.
///
/// `set_value` resets the scroll to the top, so it is always followed by
/// `set_cursor_position` on the last line to scroll back to the bottom.
///
/// ### Arguments
/// - `log_content`: The display input state to update
/// - `display`: The full text to show
/// - `window`: The active window
/// - `cx`: The application context
pub(super) fn write_log_to_bottom(
    log_content: &Entity<InputState>,
    display: &str,
    window: &mut Window,
    cx: &mut Context<Fulgur>,
) {
    let last_line = last_line_index(display);
    log_content.update(cx, |state, cx| {
        state.set_value(display, window, cx);
        state.set_cursor_position(
            Position {
                line: last_line,
                character: 0,
            },
            window,
            cx,
        );
    });
}

/// Build the read-only display `InputState` for a log view buffer.
///
/// ### Arguments
/// - `window`: The window the input is created in
/// - `cx`: The input state context
/// - `content`: The initial buffer content
/// - `soft_wrap`: Whether soft wrapping is enabled
///
/// ### Returns
/// - `InputState`: A multi-line input seeded with the content
pub(super) fn make_log_input_state(
    window: &mut Window,
    cx: &mut Context<InputState>,
    content: &str,
    soft_wrap: bool,
) -> InputState {
    InputState::new(window, cx)
        .code_editor("log")
        .line_number(true)
        .soft_wrap(soft_wrap)
        .default_value(content.to_string())
}
