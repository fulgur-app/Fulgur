//! Headless interaction tests for settings changes reaching every open editor.

use crate::fulgur::Fulgur;
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::test_support::{
    open_fulgur_with_root, open_rendered_window, register_window_in_global_manager,
};
use gpui_kit::component::input::EditorState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext};

/// Accessibility label of the switch under test, as shown on the Editor settings page.
const AUTO_CLOSE_PAIRS: &str = "Auto Close Pairs";

/// Element id gpui-component gives every boolean settings field.
const SETTING_SWITCH: &str = "check";

/// Focus a window's active editor and empty it.
///
/// ### Arguments
/// - `fulgur`: The Fulgur entity of the window
/// - `visual_cx`: The visual test context driving that window
///
/// ### Returns
/// - `Entity<EditorState>`: The active editor's content
fn focus_empty_editor(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
) -> Entity<EditorState> {
    let editor = visual_cx.update(|window, cx| {
        let editor = fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .clone();
        editor.update(cx, |editor, cx| editor.set_value("", window, cx));
        let focus = editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        editor
    });
    visual_cx.run_until_parked();
    editor
}

/// Type an opening parenthesis into a freshly emptied editor and read back the buffer.
///
/// ### Arguments
/// - `fulgur`: The Fulgur entity of the window
/// - `visual_cx`: The visual test context driving that window
///
/// ### Returns
/// - `String`: The buffer after typing, `"()"` when pairs are auto-closed and `"("` otherwise
fn type_open_paren(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) -> String {
    let editor = focus_empty_editor(fulgur, visual_cx);
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.input("(", cx);
    });
    visual_cx.run_until_parked();
    visual_cx.update(|_window, cx| editor.read(cx).value().to_string())
}

/// Read the toggle state of the Auto Close Pairs switch on the open settings page.
///
/// ### Arguments
/// - `visual_cx`: The visual test context of the window showing the settings tab
///
/// ### Returns
/// - `Some(bool)`: Whether the switch reports itself on
/// - `None`: The switch exposes no toggle state
fn auto_close_switch_checked(visual_cx: &mut VisualTestContext) -> Option<bool> {
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window
            .find_all(SETTING_SWITCH)
            .into_iter()
            .find(|switch| switch.label() == Some(AUTO_CLOSE_PAIRS))
            .expect("the Editor page must show the Auto Close Pairs switch")
            .checked()
    })
}

/// Click the Auto Close Pairs switch on the open settings page.
///
/// ### Arguments
/// - `visual_cx`: The visual test context of the window showing the settings tab
fn click_auto_close_switch(visual_cx: &mut VisualTestContext) {
    let center = visual_cx.update(|window, cx| {
        window.render_frame(cx);
        let switch = window
            .find_all(SETTING_SWITCH)
            .into_iter()
            .find(|switch| switch.label() == Some(AUTO_CLOSE_PAIRS))
            .expect("the Editor page must show the Auto Close Pairs switch");
        assert!(
            switch.visible(),
            "the switch must be on screen to be clicked"
        );
        switch.bounds().center()
    });
    visual_cx.simulate_click(center, Modifiers::none());
    visual_cx.run_until_parked();
}

#[gpui_kit::test]
fn test_settings_switch_reaches_editors_in_every_window(cx: &mut TestAppContext) {
    let (settings_window, settings_handle, mut settings_cx) = open_fulgur_with_root(cx);
    register_window_in_global_manager(cx, settings_handle.window_id(), &settings_window);
    let (other_window, other_handle, mut other_cx) = open_rendered_window(cx);
    register_window_in_global_manager(cx, other_handle.window_id(), &other_window);
    other_cx.update(|_window, cx| {
        other_window.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.force_language(cx, SupportedLanguage::Rust);
            });
        });
    });
    assert_eq!(
        type_open_paren(&other_window, &mut other_cx),
        "()",
        "a Rust editor auto-closes pairs by default"
    );

    settings_cx.update(|window, cx| {
        settings_window.update(cx, |this, cx| this.open_settings(window, cx));
    });
    settings_cx.run_until_parked();
    assert_eq!(auto_close_switch_checked(&mut settings_cx), Some(true));

    click_auto_close_switch(&mut settings_cx);

    assert_eq!(
        auto_close_switch_checked(&mut settings_cx),
        Some(false),
        "the switch must reflect the new value"
    );
    assert!(
        !other_cx.update(|_window, cx| {
            other_window
                .read(cx)
                .settings
                .editor_settings
                .auto_close_pairs
        }),
        "the other window's settings snapshot must follow"
    );
    assert_eq!(
        type_open_paren(&other_window, &mut other_cx),
        "(",
        "an editor in another window must stop auto-closing pairs"
    );
}
