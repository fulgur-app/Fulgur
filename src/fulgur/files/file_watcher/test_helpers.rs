use crate::fulgur::WindowInit;
use crate::fulgur::{
    Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
use parking_lot::Mutex as ParkingMutex;
use std::{cell::RefCell, path::PathBuf, sync::Arc};

/// Build an OS-agnostic temporary test path.
///
/// ### Arguments
/// - `file_name`: The file name to append to the platform temp directory.
///
/// ### Returns
/// - `PathBuf`: A path under `std::env::temp_dir()` suitable for cross-platform tests.
pub(super) fn temp_test_path(file_name: &str) -> PathBuf {
    std::env::temp_dir().join(file_name)
}

pub(super) fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
    cx.update(|cx| {
        gpui_component::init(cx);
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<ParkingMutex<Vec<PathBuf>>> =
            Arc::new(ParkingMutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files, None, None));
        cx.set_global(WindowManager::new());
    });
    let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
    let window = cx
        .update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, WindowInit::Empty);
                *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
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
