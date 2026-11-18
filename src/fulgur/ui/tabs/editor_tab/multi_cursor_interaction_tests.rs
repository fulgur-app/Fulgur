//! Headless interaction tests for the multi-cursor commands.

use crate::fulgur::ui::menus::build_default_key_bindings;
use crate::test_support::open_fulgur_with_root;
use gpui_kit::component::input::EditorState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Focusable, TestAppContext, VisualTestContext};

/// Three lines whose starts sit at byte offsets 0, 4 and 8.
const THREE_LINES: &str = "aaa\nbbb\nccc";

/// Byte offset of the start of the last line of `THREE_LINES`.
const LAST_LINE_START: usize = 8;

#[cfg(target_os = "macos")]
const ADD_CURSOR_ABOVE: &str = "cmd-alt-up";
#[cfg(target_os = "macos")]
const ADD_CURSOR_BELOW: &str = "cmd-alt-down";
#[cfg(target_os = "windows")]
const ADD_CURSOR_ABOVE: &str = "ctrl-alt-up";
#[cfg(target_os = "windows")]
const ADD_CURSOR_BELOW: &str = "ctrl-alt-down";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const ADD_CURSOR_ABOVE: &str = "shift-alt-up";
#[cfg(not(any(target_os = "macos", target_os = "windows")))]
const ADD_CURSOR_BELOW: &str = "shift-alt-down";

#[cfg(target_os = "macos")]
const TOGGLE_COMMAND_PALETTE: &str = "cmd-shift-p";
#[cfg(not(target_os = "macos"))]
const TOGGLE_COMMAND_PALETTE: &str = "ctrl-shift-p";

#[cfg(target_os = "macos")]
const UNDO: &str = "cmd-z";
#[cfg(not(target_os = "macos"))]
const UNDO: &str = "ctrl-z";

/// Open a window whose focused editor holds `THREE_LINES` with the caret at `caret`.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `caret`: The byte offset to place the single caret at
///
/// ### Returns
/// - `(Entity<EditorState>, VisualTestContext)`: The active editor's content and the visual test
///   context
fn setup_editor(cx: &mut TestAppContext, caret: usize) -> (Entity<EditorState>, VisualTestContext) {
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
            editor.set_selected_range(caret..caret, cx);
        });
        let focus = editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        editor
    });
    visual_cx.run_until_parked();
    (editor, visual_cx)
}

/// Press a keystroke sequence, then let the result settle.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
/// - `keystrokes`: The space separated keystrokes to press
fn press(visual_cx: &mut VisualTestContext, keystrokes: &str) {
    visual_cx.simulate_keystrokes(keystrokes);
    visual_cx.run_until_parked();
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

/// Read the editor's full text.
///
/// ### Arguments
/// - `editor`: The editor under test
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `String`: The current buffer content
fn text_of(editor: &Entity<EditorState>, visual_cx: &mut VisualTestContext) -> String {
    visual_cx.update(|_window, cx| editor.read(cx).value().to_string())
}

#[gpui_kit::test]
fn test_add_cursor_below_shortcut_types_on_every_caret(cx: &mut TestAppContext) {
    let (editor, mut visual_cx) = setup_editor(cx, 0);

    press(&mut visual_cx, ADD_CURSOR_BELOW);
    press(&mut visual_cx, ADD_CURSOR_BELOW);
    type_text(&mut visual_cx, "X");

    assert_eq!(
        text_of(&editor, &mut visual_cx),
        "Xaaa\nXbbb\nXccc",
        "typing must land at the original caret and at both added ones"
    );
}

#[gpui_kit::test]
fn test_add_cursor_above_shortcut_types_on_every_caret(cx: &mut TestAppContext) {
    let (editor, mut visual_cx) = setup_editor(cx, LAST_LINE_START);

    press(&mut visual_cx, ADD_CURSOR_ABOVE);
    press(&mut visual_cx, ADD_CURSOR_ABOVE);
    type_text(&mut visual_cx, "X");

    assert_eq!(text_of(&editor, &mut visual_cx), "Xaaa\nXbbb\nXccc");
}

#[gpui_kit::test]
fn test_undo_reverts_a_multi_cursor_edit_in_one_step(cx: &mut TestAppContext) {
    let (editor, mut visual_cx) = setup_editor(cx, 0);
    press(&mut visual_cx, ADD_CURSOR_BELOW);
    press(&mut visual_cx, ADD_CURSOR_BELOW);
    type_text(&mut visual_cx, "X");

    press(&mut visual_cx, UNDO);

    assert_eq!(
        text_of(&editor, &mut visual_cx),
        THREE_LINES,
        "one undo must revert the insertion at every caret"
    );
}

#[gpui_kit::test]
fn test_command_palette_add_cursor_below_reaches_the_editor(cx: &mut TestAppContext) {
    let (editor, mut visual_cx) = setup_editor(cx, 0);

    press(&mut visual_cx, TOGGLE_COMMAND_PALETTE);
    type_text(&mut visual_cx, "add cursor below");
    press(&mut visual_cx, "enter");

    assert!(
        visual_cx.update(|window, cx| editor.read(cx).focus_handle(cx).is_focused(window)),
        "running an editor command from the palette must hand focus back to the editor"
    );
    type_text(&mut visual_cx, "X");
    assert_eq!(
        text_of(&editor, &mut visual_cx),
        "Xaaa\nXbbb\nccc",
        "the palette command must add a caret on the next line"
    );
}

#[gpui_kit::test]
fn test_command_palette_add_cursor_above_reaches_the_editor(cx: &mut TestAppContext) {
    let (editor, mut visual_cx) = setup_editor(cx, LAST_LINE_START);

    press(&mut visual_cx, TOGGLE_COMMAND_PALETTE);
    type_text(&mut visual_cx, "add cursor above");
    press(&mut visual_cx, "enter");
    type_text(&mut visual_cx, "X");

    assert_eq!(text_of(&editor, &mut visual_cx), "aaa\nXbbb\nXccc");
}
