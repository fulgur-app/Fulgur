use std::cell::Cell;

use gpui_kit::component::{WindowExt, dialog::AlertDialog};
use gpui_kit::{App, FocusHandle, Window};

/// Opening alert dialogs whose initial focus is set on their first render only.
pub trait OpenAlertDialogWithFocus {
    /// Open an alert dialog that focuses `initial_focus` on its first render
    ///
    /// ### Arguments
    /// - `initial_focus`: The focus handle of the control that starts focused
    /// - `cx`: The application context
    /// - `build`: The dialog builder, run on every render like the one given to
    ///   `open_alert_dialog`
    fn open_alert_dialog_with_focus<F>(
        &mut self,
        initial_focus: FocusHandle,
        cx: &mut App,
        build: F,
    ) where
        F: Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static;
}

impl OpenAlertDialogWithFocus for Window {
    fn open_alert_dialog_with_focus<F>(
        &mut self,
        initial_focus: FocusHandle,
        cx: &mut App,
        build: F,
    ) where
        F: Fn(AlertDialog, &mut Window, &mut App) -> AlertDialog + 'static,
    {
        let pending_focus = Cell::new(Some(initial_focus));
        self.open_alert_dialog(cx, move |dialog, window, cx| {
            if let Some(focus_handle) = pending_focus.take() {
                window.focus(&focus_handle, cx);
            }
            build(dialog, window, cx)
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use gpui_kit::component::input::{Input, InputState};
    use gpui_kit::{
        AppContext, Entity, Focusable, ParentElement, TestAppContext, VisualTestContext,
    };

    use super::OpenAlertDialogWithFocus;
    use crate::test_support::setup_fulgur_with_root;

    /// Draw a frame of the window so the dialog layer runs its builder again.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window showing the dialog
    fn draw_frame(visual_cx: &mut VisualTestContext) {
        visual_cx.run_until_parked();
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        visual_cx.run_until_parked();
    }

    /// Whether the input currently holds the window focus.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window showing the dialog
    /// - `input`: The input to inspect
    ///
    /// ### Returns
    /// - `bool`: `true` when the input is focused
    fn is_focused(visual_cx: &mut VisualTestContext, input: &Entity<InputState>) -> bool {
        visual_cx.update(|window, cx| input.read(cx).focus_handle(cx).is_focused(window))
    }

    #[gpui_kit::test]
    fn test_initial_focus_is_applied_once_and_survives_tab(cx: &mut TestAppContext) {
        let (_fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let first = visual_cx.update(|window, cx| cx.new(|cx| InputState::new(window, cx)));
        let second = visual_cx.update(|window, cx| cx.new(|cx| InputState::new(window, cx)));

        let first_for_dialog = first.clone();
        let second_for_dialog = second.clone();
        visual_cx.update(|window, cx| {
            let initial_focus = first.read(cx).focus_handle(cx);
            window.open_alert_dialog_with_focus(initial_focus, cx, move |dialog, _, _| {
                dialog
                    .child(Input::new(&first_for_dialog))
                    .child(Input::new(&second_for_dialog))
            });
        });
        draw_frame(&mut visual_cx);
        assert!(
            is_focused(&mut visual_cx, &first),
            "the initial control should be focused when the dialog opens"
        );

        visual_cx.update(|window, cx| window.focus(&second.read(cx).focus_handle(cx), cx));
        draw_frame(&mut visual_cx);
        draw_frame(&mut visual_cx);
        assert!(
            is_focused(&mut visual_cx, &second),
            "moving the focus inside the dialog must survive later renders"
        );
        assert!(!is_focused(&mut visual_cx, &first));
    }
}
