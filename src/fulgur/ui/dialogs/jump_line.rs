use std::ops::DerefMut;

use gpui::{Context, Focusable, ParentElement, Styled, Window, div, px};
use gpui_component::{WindowExt, button::ButtonVariant, dialog::DialogButtonProps, input::Input};

use crate::fulgur::{Fulgur, editor_tab};

impl Fulgur {
    /// Show the jump to line dialog
    ///
    /// ### Arguments
    /// - `window`: The window to show the dialog in
    /// - `cx`: The application context
    pub fn show_jump_to_line_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.jump_to_line_input.update(cx, |input_state, cx| {
            input_state.set_value("", window, cx);
            cx.notify();
        });
        let jump_to_line_input = self.jump_to_line_input.clone();
        let entity = cx.entity().clone();
        self.jump_to_line_dialog_open = true;
        window.open_alert_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = jump_to_line_input.read(cx).focus_handle(cx);
            window.focus(&focus_handle, cx);
            let jump_to_line_input_clone = jump_to_line_input.clone();
            let entity_for_ok = entity.clone();
            let entity_for_cancel = entity.clone();
            modal
                .title(div().text_size(px(16.)).child("Jump to line..."))
                .keyboard(true)
                .button_props(
                    DialogButtonProps::default()
                        .show_cancel(true)
                        .cancel_text("Cancel")
                        .cancel_variant(ButtonVariant::Secondary)
                        .ok_text("Jump")
                        .ok_variant(ButtonVariant::Primary),
                )
                .overlay_closable(true)
                .close_button(false)
                .child(Input::new(&jump_to_line_input))
                .on_ok(move |_, _window, cx| {
                    let text = jump_to_line_input_clone.read(cx).value();
                    let jump_result = editor_tab::extract_line_number(&text);
                    let is_ok = jump_result.is_ok();
                    entity_for_ok.update(cx, |this, cx| {
                        if let Ok(jump) = jump_result {
                            this.pending_jump = Some(jump);
                            this.jump_to_line_dialog_open = false;
                            cx.notify();
                        } else {
                            this.pending_jump = None;
                        }
                    });

                    is_ok
                })
                .on_cancel(move |_, _, cx| {
                    entity_for_cancel.update(cx, |this, cx| {
                        this.jump_to_line_dialog_open = false;
                        cx.notify();
                    });
                    true
                })
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur,
        settings::Settings,
        shared_state::SharedAppState,
        ui::tabs::{editor_tab::Jump, tab::Tab},
        window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    use parking_lot::Mutex;
    use std::{cell::RefCell, rc::Rc, sync::Arc};

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

    // ========== show_jump_to_line_dialog tests ==========

    #[gpui::test]
    fn test_show_jump_to_line_dialog_sets_open_flag(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        // The render loop resets the flag to true on each render pass, so force it
        // to false before calling show_jump_to_line_dialog to verify the method sets it.
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.jump_to_line_dialog_open = false;
                this.show_jump_to_line_dialog(window, cx);
            });
        });
        let after = fulgur.read_with(&visual_cx, |this, _| this.jump_to_line_dialog_open);
        assert!(
            after,
            "show_jump_to_line_dialog should set the open flag to true"
        );
    }

    // ========== handle_pending_jump_to_line tests ==========

    #[gpui::test]
    fn test_handle_pending_jump_consumes_pending_jump(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_jump = Some(Jump {
                    line: 0,
                    character: None,
                });
                this.handle_pending_jump_to_line(window, cx);
            });
        });
        let pending = fulgur.read_with(&visual_cx, |this, _| this.pending_jump.is_some());
        assert!(!pending, "pending_jump should be consumed after handling");
    }

    #[gpui::test]
    fn test_handle_pending_jump_applies_cursor_line(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        // Set multi-line content in the active editor tab
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(editor_tab)) = this.tabs.first_mut() {
                    editor_tab.content.update(cx, |input, cx| {
                        input.set_value("line one\nline two\nline three", window, cx);
                    });
                }
            });
        });
        // Jump to line 2 (0-indexed: line = 1)
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_jump = Some(Jump {
                    line: 1,
                    character: None,
                });
                this.handle_pending_jump_to_line(window, cx);
            });
        });
        let cursor_line = fulgur.read_with(&visual_cx, |this, cx| {
            this.tabs
                .first()
                .and_then(|t| t.as_editor())
                .map(|e| e.content.read(cx).cursor_position().line)
        });
        assert_eq!(cursor_line, Some(1), "cursor should be on line index 1");
    }

    #[gpui::test]
    fn test_handle_pending_jump_is_noop_without_pending_jump(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        // No pending_jump set, handler should do nothing
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.handle_pending_jump_to_line(window, cx);
            });
        });
        let cursor_line = fulgur.read_with(&visual_cx, |this, cx| {
            this.tabs
                .first()
                .and_then(|t| t.as_editor())
                .map(|e| e.content.read(cx).cursor_position().line)
        });
        assert_eq!(
            cursor_line,
            Some(0),
            "cursor should remain at line 0 when no jump pending"
        );
    }

    #[gpui::test]
    fn test_handle_pending_jump_is_noop_without_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.pending_jump = Some(Jump {
                    line: 5,
                    character: None,
                });
                this.handle_pending_jump_to_line(window, cx);
            });
        });
        // pending_jump was set but there was no active tab, it should have been consumed (taken) but not applied
        let pending = fulgur.read_with(&visual_cx, |this, _| this.pending_jump.is_some());
        assert!(
            !pending,
            "pending_jump should be consumed even without an active tab"
        );
    }
}
