//! Read-only display-buffer helpers for the log view.

use gpui::{Context, Entity, Window};
use gpui_component::input::{InputState, Position, RopeExt};

use super::LOG_LINE_CAP;
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

/// Append text to a log buffer with incremental edits and snap to the bottom.
///
/// ### Arguments
/// - `log_content`: The display input state to update
/// - `text`: The newly appended text (may be empty; only the snap happens)
/// - `log_full`: Whether the line cap is lifted for this tab
/// - `window`: The active window
/// - `cx`: The application context
///
/// ### Returns
/// - `bool`: Whether the line cap dropped lines from the front on this write
pub(super) fn append_log_to_bottom(
    log_content: &Entity<InputState>,
    text: &str,
    log_full: bool,
    window: &mut Window,
    cx: &mut Context<Fulgur>,
) -> bool {
    log_content.update(cx, |state, cx| {
        if !text.is_empty() {
            let end = state.text().len();
            state.set_selected_range(end..end, cx);
            state.insert(text, window, cx);
        }
        let mut dropped = false;
        if !log_full {
            let newline_count = state.text().lines_len().saturating_sub(1);
            if newline_count > LOG_LINE_CAP {
                let cut = state.text().line_start_offset(newline_count - LOG_LINE_CAP);
                state.set_selected_range(0..cut, cx);
                state.replace("", window, cx);
                dropped = true;
            }
        }
        let last_line =
            u32::try_from(state.text().lines_len().saturating_sub(1)).unwrap_or(u32::MAX);
        state.set_cursor_position(
            Position {
                line: last_line,
                character: 0,
            },
            window,
            cx,
        );
        dropped
    })
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
