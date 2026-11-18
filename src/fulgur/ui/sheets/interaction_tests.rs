//! Headless interaction tests for sheet focus and dismissal, through the language sheet.

use crate::fulgur::Fulgur;
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::test_support::open_fulgur_with_root;
use gpui_kit::component::WindowExt;
use gpui_kit::component::input::EditorState;
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Focusable, Modifiers, TestAppContext, VisualTestContext, point, px};

/// Open a window with a focused editor, then open the language sheet from the status bar.
///
/// ### Arguments
/// - `cx`: The test application context
///
/// ### Returns
/// - `(Entity<Fulgur>, Entity<EditorState>, VisualTestContext)`: The Fulgur entity, the active
///   editor's content and the visual test context
fn open_language_sheet(
    cx: &mut TestAppContext,
) -> (Entity<Fulgur>, Entity<EditorState>, VisualTestContext) {
    let (fulgur, _handle, mut visual_cx) = open_fulgur_with_root(cx);
    let editor = visual_cx.update(|window, cx| {
        let editor = fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .content
            .clone();
        let focus = editor.read(cx).focus_handle(cx);
        window.focus(&focus, cx);
        editor
    });
    visual_cx.run_until_parked();
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click("status-language", cx);
    });
    visual_cx.run_until_parked();
    (fulgur, editor, visual_cx)
}

/// Whether the window currently shows a sheet.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `bool`: `true` when a sheet is open
fn has_active_sheet(visual_cx: &mut VisualTestContext) -> bool {
    visual_cx.update(WindowExt::has_active_sheet)
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

/// Read the active editor tab's language.
///
/// ### Arguments
/// - `fulgur`: The Fulgur entity under test
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `SupportedLanguage`: The language of the active editor tab
fn active_language(
    fulgur: &Entity<Fulgur>,
    visual_cx: &mut VisualTestContext,
) -> SupportedLanguage {
    visual_cx.update(|_window, cx| {
        fulgur
            .read(cx)
            .get_active_editor_tab(cx)
            .expect("expected an active editor tab")
            .language
    })
}

/// Find the first language row that is on screen and differs from `current`.
///
/// ### Arguments
/// - `current`: The language the active tab already has
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `(usize, SupportedLanguage)`: The row's element id index and its language
fn first_visible_other_language(
    current: SupportedLanguage,
    visual_cx: &mut VisualTestContext,
) -> (usize, SupportedLanguage) {
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        SupportedLanguage::all()
            .iter()
            .copied()
            .enumerate()
            .find(|(index, language)| {
                *language != current
                    && window
                        .try_find(("select-language", *index))
                        .is_some_and(|row| row.visible())
            })
            .expect("the sheet must show at least one other language")
    })
}

#[gpui_kit::test]
fn test_status_bar_opens_the_language_sheet_and_moves_focus_into_it(cx: &mut TestAppContext) {
    let (_fulgur, editor, mut visual_cx) = open_language_sheet(cx);

    assert!(has_active_sheet(&mut visual_cx));
    assert!(
        !editor_focused(&editor, &mut visual_cx),
        "an open sheet must take focus away from the editor"
    );
}

#[gpui_kit::test]
fn test_picking_a_language_applies_it_and_refocuses_the_editor(cx: &mut TestAppContext) {
    let (fulgur, editor, mut visual_cx) = open_language_sheet(cx);
    let current = active_language(&fulgur, &mut visual_cx);
    let (index, target) = first_visible_other_language(current, &mut visual_cx);

    visual_cx.update(|window, cx| {
        window.click(("select-language", index), cx);
    });
    visual_cx.run_until_parked();

    assert_eq!(active_language(&fulgur, &mut visual_cx), target);
    assert!(
        !has_active_sheet(&mut visual_cx),
        "picking must close the sheet"
    );
    assert!(
        editor_focused(&editor, &mut visual_cx),
        "closing the sheet must return focus to the editor it was opened from"
    );
}

#[gpui_kit::test]
fn test_escape_dismisses_the_sheet_without_changing_the_language(cx: &mut TestAppContext) {
    let (fulgur, editor, mut visual_cx) = open_language_sheet(cx);
    let before = active_language(&fulgur, &mut visual_cx);

    visual_cx.simulate_keystrokes("escape");
    visual_cx.run_until_parked();

    assert!(!has_active_sheet(&mut visual_cx), "Escape must dismiss");
    assert_eq!(active_language(&fulgur, &mut visual_cx), before);
    assert!(editor_focused(&editor, &mut visual_cx));
}

#[gpui_kit::test]
fn test_clicking_the_backdrop_dismisses_the_sheet(cx: &mut TestAppContext) {
    let (fulgur, editor, mut visual_cx) = open_language_sheet(cx);
    let before = active_language(&fulgur, &mut visual_cx);

    // The sheet is 400 px wide on the left; this point is on the overlay to its right.
    visual_cx.simulate_click(point(px(1000.0), px(400.0)), Modifiers::none());
    visual_cx.run_until_parked();

    assert!(
        !has_active_sheet(&mut visual_cx),
        "the overlay must dismiss the sheet"
    );
    assert_eq!(active_language(&fulgur, &mut visual_cx), before);
    assert!(editor_focused(&editor, &mut visual_cx));
}
