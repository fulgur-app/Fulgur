//! Headless interaction tests for toggling the Markdown preview from the status bar.

use crate::fulgur::Fulgur;
use crate::fulgur::languages::supported_languages::SupportedLanguage;
use crate::fulgur::settings::MarkdownPreviewMode;
use crate::fulgur::ui::tabs::tab::{Tab, TabId};
use crate::test_support::{open_rendered_window, setup_test_globals_with};
use gpui_kit::test::TestWindowExt;
use gpui_kit::{Entity, Focusable, TestAppContext, VisualTestContext};

/// Element id of the status bar button that toggles the preview.
const PREVIEW_TOGGLE: &str = "status-preview";

/// Element id of the editor pane, rendered only while the side panel is shown.
const PANEL_EDITOR: &str = "markdown-editor";

/// Open a rendered window whose active tab is a Markdown editor with its preview hidden.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `mode`: How the preview is presented
///
/// ### Returns
/// - `(Entity<Fulgur>, TabId, VisualTestContext)`: The Fulgur entity, the Markdown tab id and the
///   visual test context
fn open_markdown_tab(
    cx: &mut TestAppContext,
    mode: MarkdownPreviewMode,
) -> (Entity<Fulgur>, TabId, VisualTestContext) {
    setup_test_globals_with(cx, |settings| {
        settings.editor_settings.markdown_settings.preview_mode = mode;
        settings
            .editor_settings
            .markdown_settings
            .show_markdown_preview = false;
    });
    let (fulgur, _handle, mut visual_cx) = open_rendered_window(cx);
    let tab_id = visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.force_language(cx, SupportedLanguage::Markdown);
            });
            cx.notify();
            this.active_tab_id.expect("expected an active tab")
        })
    });
    visual_cx.run_until_parked();
    (fulgur, tab_id, visual_cx)
}

/// Click an element after rendering a fresh frame, then let the result settle.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
/// - `id`: The element id to click
fn click(visual_cx: &mut VisualTestContext, id: &'static str) {
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(id, cx);
    });
    visual_cx.run_until_parked();
}

/// Read the toggle state the preview button reports to accessibility.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `Some(bool)`: Whether the button reports itself toggled on
/// - `None`: The button exposes no toggle state
fn preview_toggle_checked(visual_cx: &mut VisualTestContext) -> Option<bool> {
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.find(PREVIEW_TOGGLE).checked()
    })
}

/// Whether the side panel layout is currently in the render tree.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `bool`: `true` when the editor is rendered inside the side panel layout
fn side_panel_shown(visual_cx: &mut VisualTestContext) -> bool {
    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.try_find(PANEL_EDITOR).is_some()
    })
}

/// Count the dedicated preview tabs open in the window.
///
/// ### Arguments
/// - `fulgur`: The Fulgur entity under test
/// - `visual_cx`: The visual test context driving the window
///
/// ### Returns
/// - `usize`: The number of Markdown preview tabs
fn preview_tab_count(fulgur: &Entity<Fulgur>, visual_cx: &mut VisualTestContext) -> usize {
    visual_cx.update(|_window, cx| {
        fulgur
            .read(cx)
            .tabs
            .iter()
            .filter(|tab| matches!(tab.read(cx), Tab::MarkdownPreview(_)))
            .count()
    })
}

#[gpui_kit::test]
fn test_preview_toggle_shows_then_hides_the_side_panel(cx: &mut TestAppContext) {
    let (fulgur, _tab_id, mut visual_cx) = open_markdown_tab(cx, MarkdownPreviewMode::Panel);
    assert!(!side_panel_shown(&mut visual_cx));
    assert_eq!(preview_toggle_checked(&mut visual_cx), Some(false));

    click(&mut visual_cx, PREVIEW_TOGGLE);

    assert!(
        side_panel_shown(&mut visual_cx),
        "clicking Preview must open the side panel"
    );
    assert_eq!(
        preview_toggle_checked(&mut visual_cx),
        Some(true),
        "the button must report the panel as toggled on"
    );
    assert_eq!(
        preview_tab_count(&fulgur, &mut visual_cx),
        0,
        "panel mode must never open a dedicated preview tab"
    );

    click(&mut visual_cx, PREVIEW_TOGGLE);

    assert!(
        !side_panel_shown(&mut visual_cx),
        "clicking Preview again must close the side panel"
    );
    assert_eq!(preview_toggle_checked(&mut visual_cx), Some(false));
}

#[gpui_kit::test]
fn test_toggling_the_side_panel_keeps_the_editor_focused(cx: &mut TestAppContext) {
    let (fulgur, _tab_id, mut visual_cx) = open_markdown_tab(cx, MarkdownPreviewMode::Panel);
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

    for state in ["open", "closed"] {
        click(&mut visual_cx, PREVIEW_TOGGLE);
        assert!(
            visual_cx.update(|window, cx| editor.read(cx).focus_handle(cx).is_focused(window)),
            "the editor must keep focus once the side panel is {state}"
        );
    }
}

#[gpui_kit::test]
fn test_leaving_markdown_closes_the_side_panel(cx: &mut TestAppContext) {
    let (fulgur, _tab_id, mut visual_cx) = open_markdown_tab(cx, MarkdownPreviewMode::Panel);
    click(&mut visual_cx, PREVIEW_TOGGLE);
    assert!(side_panel_shown(&mut visual_cx));

    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            this.update_active_editor_tab(cx, |editor, cx| {
                editor.force_language(cx, SupportedLanguage::Plain);
            });
            cx.notify();
        });
    });
    visual_cx.run_until_parked();

    assert!(
        !side_panel_shown(&mut visual_cx),
        "a tab that is no longer Markdown must not keep the side panel"
    );
    assert!(
        visual_cx.update(|window, cx| {
            window.render_frame(cx);
            window.try_find(PREVIEW_TOGGLE).is_none()
        }),
        "the Preview toggle is only offered for Markdown"
    );
}

#[gpui_kit::test]
fn test_preview_toggle_opens_then_closes_the_dedicated_tab(cx: &mut TestAppContext) {
    let (fulgur, source_id, mut visual_cx) =
        open_markdown_tab(cx, MarkdownPreviewMode::DedicatedTab);
    assert_eq!(preview_tab_count(&fulgur, &mut visual_cx), 0);

    click(&mut visual_cx, PREVIEW_TOGGLE);

    assert_eq!(
        preview_tab_count(&fulgur, &mut visual_cx),
        1,
        "clicking Preview must open a dedicated preview tab"
    );
    assert!(
        !side_panel_shown(&mut visual_cx),
        "dedicated tab mode must never split the editor"
    );
    let active_is_preview = visual_cx.update(|_window, cx| {
        matches!(
            fulgur.read(cx).active_tab(cx),
            Some(Tab::MarkdownPreview(preview)) if preview.source_tab_id == source_id
        )
    });
    assert!(active_is_preview, "the new preview tab must become active");

    visual_cx.update(|window, cx| {
        window.render_frame(cx);
        window.click(("tab", source_id.0), cx);
    });
    visual_cx.run_until_parked();
    assert_eq!(
        preview_toggle_checked(&mut visual_cx),
        Some(true),
        "back on the source tab, the button must report its preview as open"
    );

    click(&mut visual_cx, PREVIEW_TOGGLE);

    assert_eq!(
        preview_tab_count(&fulgur, &mut visual_cx),
        0,
        "clicking Preview again must close the dedicated preview tab"
    );
}
