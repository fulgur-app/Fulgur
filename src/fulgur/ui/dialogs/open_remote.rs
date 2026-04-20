use std::ops::DerefMut;

use gpui::{AppContext, Context, Focusable, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input,
    notification::NotificationType,
};

use crate::fulgur::{Fulgur, sync::ssh::url::parse_remote_url};

impl Fulgur {
    /// Show the open remote file dialog.
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_open_remote_dialog(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity().clone();
        let input = cx.new(|cx| {
            gpui_component::input::InputState::new(window, cx)
                .placeholder("ssh://user@host/path/to/file")
        });
        let input_for_ok = input.clone();
        window.open_alert_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            let entity_ok = entity.clone();
            let input_ok = input_for_ok.clone();
            modal
                .title(div().text_size(px(16.)).child("Open remote file..."))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Open")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(false)
                .close_button(false)
                .child(Input::new(&input))
                .on_ok(move |_, window: &mut Window, cx| {
                    let url = input_ok.read(cx).value().to_string();
                    match parse_remote_url(&url) {
                        Ok(spec) => {
                            entity_ok.update(cx, |this, cx| {
                                this.do_open_remote_file(window, cx, spec);
                            });
                            true
                        }
                        Err(err) => {
                            window.push_notification(
                                (
                                    NotificationType::Error,
                                    SharedString::from(err.user_message()),
                                ),
                                cx,
                            );
                            false
                        }
                    }
                })
                .on_cancel(|_, _, _| true)
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    use parking_lot::Mutex;
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    /// Set up a minimal Fulgur instance inside a test window.
    ///
    /// ### Arguments
    /// - `cx`: The test app context
    ///
    /// ### Returns
    /// - `(Entity<Fulgur>, VisualTestContext)`: The Fulgur entity and its visual test context
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_global(SharedAppState::new(
                Settings::new(),
                Arc::new(Mutex::new(Vec::new())),
            ));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: Rc<RefCell<Option<Entity<Fulgur>>>> = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&fulgur_slot);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let fulgur = fulgur_slot
            .borrow_mut()
            .take()
            .expect("expected fulgur entity");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        (fulgur, visual_cx)
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_show_open_remote_dialog_does_not_panic(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.show_open_remote_dialog(window, cx);
            });
        });
    }
}
