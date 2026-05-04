#[cfg(feature = "gpui-test-support")]
use crate::fulgur::{
    Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
};
#[cfg(feature = "gpui-test-support")]
use gpui::WindowId;
#[cfg(feature = "gpui-test-support")]
use gpui::{
    AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext, Window,
    WindowOptions, div,
};
#[cfg(feature = "gpui-test-support")]
use parking_lot::Mutex;
#[cfg(feature = "gpui-test-support")]
use std::{cell::RefCell, path::PathBuf, sync::Arc};

#[cfg(feature = "gpui-test-support")]
pub struct EmptyView;

#[cfg(feature = "gpui-test-support")]
impl Render for EmptyView {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
    }
}

/// Build an OS-agnostic temporary test path.
///
/// ### Parameters
/// - `file_name`: The file name to append to the platform temp directory.
///
/// ### Returns
/// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests.
#[cfg(feature = "gpui-test-support")]
pub fn temp_test_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

#[cfg(feature = "gpui-test-support")]
pub fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
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
            cx.open_window(WindowOptions::default(), |window, cx| {
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

#[cfg(feature = "gpui-test-support")]
pub fn setup_test_globals(cx: &mut TestAppContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files));
        cx.set_global(WindowManager::new());
    });
}

#[cfg(feature = "gpui-test-support")]
pub fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
    let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    cx.update(|cx| {
        cx.open_window(WindowOptions::default(), |window, cx| {
            let window_id = window.window_handle().window_id();
            let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
            *window_id_slot.borrow_mut() = Some(window_id);
            *fulgur_slot.borrow_mut() = Some(fulgur.clone());
            cx.new(|_| EmptyView)
        })
        .expect("failed to open test window");
    });
    (
        window_id_slot
            .into_inner()
            .expect("failed to capture test window id"),
        fulgur_slot
            .into_inner()
            .expect("failed to capture test Fulgur entity"),
    )
}

#[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
pub fn invoke_process_pending_files_from_macos(
    cx: &mut TestAppContext,
    window_id: WindowId,
    fulgur: &Entity<Fulgur>,
) {
    cx.update(|cx| {
        for handle in cx.windows() {
            if handle.window_id() == window_id {
                handle
                    .update(cx, |_, window, cx| {
                        fulgur.update(cx, |this, cx| {
                            this.process_pending_files_from_macos(window, cx);
                        });
                    })
                    .expect("failed to run process_pending_files_from_macos on test window");
                return;
            }
        }
        panic!("failed to locate target test window by id");
    });
}
