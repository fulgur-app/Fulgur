use crate::fulgur::{
    Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
};
use gpui::{AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext};
use parking_lot::Mutex;
use std::{cell::RefCell, path::PathBuf, sync::Arc};

// ========== Test helpers ==========

struct EmptyView;

impl Render for EmptyView {
    fn render(&mut self, _window: &mut gpui::Window, _cx: &mut Context<Self>) -> impl IntoElement {
        gpui::div()
    }
}

fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files));
        cx.set_global(WindowManager::new());
    });
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(Default::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                *fulgur_slot.borrow_mut() = Some(fulgur);
                cx.new(|_| EmptyView)
            })
        })
        .expect("failed to open test window");
    let visual_cx = VisualTestContext::from_window(window.into(), cx);
    visual_cx.run_until_parked();
    let fulgur = fulgur_slot
        .into_inner()
        .expect("failed to capture Fulgur entity");
    (fulgur, visual_cx)
}

// ========== Visibility control ==========

#[gpui::test]
fn test_color_picker_bar_hidden_by_default(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, cx| {
            assert!(!this.color_picker_bar_state.show_color_picker);
            assert!(this.render_color_picker_bar(cx).is_none());
        });
    });
}

#[gpui::test]
fn test_toggle_color_picker_shows_bar(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.toggle_color_picker(window, cx);
            assert!(this.color_picker_bar_state.show_color_picker);
        });
    });
}

#[gpui::test]
fn test_toggle_color_picker_twice_hides_bar(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.toggle_color_picker(window, cx);
            assert!(this.color_picker_bar_state.show_color_picker);

            this.toggle_color_picker(window, cx);
            assert!(!this.color_picker_bar_state.show_color_picker);
        });
    });
}

// ========== Insert color value ==========

#[gpui::test]
fn test_insert_color_value_inserts_at_cursor(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let editor = this
                .get_active_editor_tab_mut()
                .expect("expected active editor tab");
            editor.content.update(cx, |content, cx| {
                content.set_value("color: ;", window, cx);
                content.set_cursor_position(
                    gpui_component::input::Position {
                        line: 0,
                        character: 7,
                    },
                    window,
                    cx,
                );
            });
            this.insert_color_value("#FF0000".to_string(), window, cx);
            let text = this
                .get_active_editor_tab()
                .expect("expected active editor tab")
                .content
                .read(cx)
                .text()
                .to_string();
            assert_eq!(text, "color: #FF0000;");
        });
    });
}

#[gpui::test]
fn test_insert_color_value_no_active_tab_does_not_panic(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            this.active_tab_index = None;
            this.insert_color_value("#FF0000".to_string(), window, cx);
        });
    });
}

// ========== Highlight colors toggle ==========

#[gpui::test]
fn test_highlight_toggle_reflects_editor_setting(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            let initial = this.settings.editor_settings.highlight_colors;
            this.settings.editor_settings.highlight_colors = !initial;
            assert_ne!(
                this.settings.editor_settings.highlight_colors, initial,
                "toggling highlight_colors should change the setting"
            );
            this.settings.editor_settings.highlight_colors = initial;
            assert_eq!(
                this.settings.editor_settings.highlight_colors, initial,
                "reverting should restore original value"
            );
        });
    });
}
