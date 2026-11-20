//! Headless interaction tests for modal dialog focus trapping and dismissal.

use crate::fulgur::Fulgur;
use crate::fulgur::ui::menus::build_default_key_bindings;
use crate::test_support::{has_active_dialog, open_fulgur_with_root};
use gpui_kit::component::input::EditorState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, point, px};

/// Three lines whose starts sit at byte offsets 0, 4 and 8.
const THREE_LINES: &str = "aaa\nbbb\nccc";

/// Byte offset of the start of the second line of `THREE_LINES`.
const SECOND_LINE_START: usize = 4;

#[cfg(target_os = "macos")]
const TOGGLE_COMMAND_PALETTE: &str = "cmd-shift-p";
#[cfg(not(target_os = "macos"))]
const TOGGLE_COMMAND_PALETTE: &str = "ctrl-shift-p";

/// Open a window whose focused editor holds `THREE_LINES`, then open Jump to line from the
/// status bar.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, Entity<EditorState>, VisualTestContext)`: The Fulgur entity, the active
///   editor's content and the visual test context
fn open_jump_to_line(
    cx: &mut TestAppContext,
) -> (Entity<Fulgur>, Entity<EditorState>, VisualTestContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    visual_cx.update(|_window, cx| cx.bind_keys(build_default_key_bindings()));
    let editor = visual_cx.update(|window, cx| {
        let editor = fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .clone();
        editor.update(cx, |editor, cx| {
            editor.set_value(THREE_LINES, window, cx);
            editor.set_selected_range(0..0, cx);
        });
        let focus = editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        editor
    });
    visual_cx.run_until_parked();
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("status-jump-to-line", cx);
    });
    visual_cx.run_until_parked();
    (fulgur, editor, visual_cx)
}

/// Whether the editor holds keyboard focus.
///
/// ### Arguments
/// - `editor`: The editor under test
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `bool`: `true` when the editor is focused
fn editor_focused(editor: &Entity<EditorState>, visual_cx: &mut VisualTestContext) -> bool {
    visual_cx.update(|window, cx| editor.read(cx).focus_handle(cx).is_focused(window))
}

/// Type text into the focused element, then let the result settle.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
/// - `text`: The text to type
fn type_text(visual_cx: &mut VisualTestContext, text: &str) {
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.input(text, cx);
    });
    visual_cx.run_until_parked();
}

#[gpui_kit::test]
fn test_status_bar_opens_jump_to_line_with_its_input_focused(cx: &mut TestAppContext) {
    let (fulgur, _editor, mut visual_cx) = open_jump_to_line(cx);

    assert!(has_active_dialog(&mut visual_cx));
    let input_focused = visual_cx.update(|window, cx| {
        fulgur
            .read(cx)
            .jump_to_line_input
            .read(cx)
            .focus_handle(cx)
            .is_focused(window)
    });
    assert!(input_focused, "the dialog must take keyboard focus");
}

#[gpui_kit::test]
fn test_typing_under_a_dialog_never_reaches_the_editor(cx: &mut TestAppContext) {
    let (fulgur, editor, mut visual_cx) = open_jump_to_line(cx);

    type_text(&mut visual_cx, "2");

    let input = visual_cx.update(|_window, cx| {
        fulgur
            .read(cx)
            .jump_to_line_input
            .read(cx)
            .value()
            .to_string()
    });
    assert_eq!(input, "2", "typed text must go to the dialog input");
    assert_eq!(
        visual_cx.update(|_window, cx| editor.read(cx).value().to_string()),
        THREE_LINES,
        "the editor under the dialog must not receive keystrokes"
    );
}

#[gpui_kit::test]
fn test_window_shortcuts_stay_inert_under_a_dialog(cx: &mut TestAppContext) {
    let (fulgur, _editor, mut visual_cx) = open_jump_to_line(cx);

    visual_cx.simulate_keystrokes(TOGGLE_COMMAND_PALETTE);
    visual_cx.run_until_parked();

    assert!(
        !visual_cx.update(|_window, cx| fulgur.read(cx).command_palette.read(cx).is_open()),
        "a tab-scoped shortcut must not fire while a dialog holds focus"
    );
    assert!(has_active_dialog(&mut visual_cx));
}

#[gpui_kit::test]
fn test_clicking_the_backdrop_keeps_the_dialog_open(cx: &mut TestAppContext) {
    let (_fulgur, _editor, mut visual_cx) = open_jump_to_line(cx);

    visual_cx.simulate_click(point(px(5.0), px(5.0)), Modifiers::none());
    visual_cx.run_until_parked();

    assert!(
        has_active_dialog(&mut visual_cx),
        "dialogs are dismissed by their buttons or Escape, never by a backdrop click"
    );
}

#[gpui_kit::test]
fn test_enter_jumps_and_hands_focus_back_to_the_editor(cx: &mut TestAppContext) {
    let (_fulgur, editor, mut visual_cx) = open_jump_to_line(cx);

    type_text(&mut visual_cx, "2");
    visual_cx.simulate_keystrokes("enter");
    visual_cx.run_until_parked();

    assert!(!has_active_dialog(&mut visual_cx), "Enter must confirm");
    assert!(
        editor_focused(&editor, &mut visual_cx),
        "closing the dialog must return focus to the editor it was opened from"
    );
    assert_eq!(
        visual_cx.update(|_window, cx| editor.read(cx).cursor()),
        SECOND_LINE_START,
        "the caret must land at the start of the requested line"
    );
}

#[gpui_kit::test]
fn test_escape_dismisses_without_jumping_and_refocuses_the_editor(cx: &mut TestAppContext) {
    let (_fulgur, editor, mut visual_cx) = open_jump_to_line(cx);

    type_text(&mut visual_cx, "2");
    visual_cx.simulate_keystrokes("escape");
    visual_cx.run_until_parked();

    assert!(!has_active_dialog(&mut visual_cx), "Escape must dismiss");
    assert!(editor_focused(&editor, &mut visual_cx));
    assert_eq!(
        visual_cx.update(|_window, cx| editor.read(cx).cursor()),
        0,
        "a dismissed jump must leave the caret where it was"
    );
}
