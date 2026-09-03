#[cfg(feature = "gpui-test-support")]
pub use crate::fulgur::test_support::{
    EmptyView, open_window_with_fulgur, setup_fulgur, setup_fulgur_with_root, setup_test_globals,
    temp_test_path,
};

#[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
use crate::fulgur::Fulgur;
#[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
use gpui::{Entity, TestAppContext, WindowId};

/// Drive `process_pending_files_from_macos` on a specific test window.
///
/// ### Arguments
/// - `cx`: The test application context
/// - `window_id`: The id of the window to run the pass on
/// - `fulgur`: The `Fulgur` entity hosted by that window
#[cfg(all(feature = "gpui-test-support", target_os = "macos"))]
#[allow(clippy::missing_panics_doc)]
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
