//! Shared helpers for GPUI-backed tests.

use gpui_kit::App;

/// Initialize GPUI Kit for a test application context.
///
/// ### Arguments
/// - `cx`: The application context to initialize
pub fn init_test_app(cx: &mut App) {
    gpui_kit::init(cx);
    // Keeps simulated input deterministic
    cx.set_reduce_motion(true);
}
