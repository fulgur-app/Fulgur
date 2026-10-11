pub(crate) use crate::test_support::{setup_fulgur_with_root as setup_fulgur, temp_test_path};

use super::suppression::FILE_WATCH_DEBOUNCE;
use gpui_kit::VisualTestContext;
use std::time::Duration;

/// Advance the fake clock, then run every task it woke, including the reloads
/// started by the file checks that fired.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
/// - `elapsed`: How much time passes
pub(crate) fn advance_file_watch_clock(visual_cx: &mut VisualTestContext, elapsed: Duration) {
    visual_cx.background_executor.advance_clock(elapsed);
    visual_cx.run_until_parked();
}

/// Let the watch events delivered so far go quiet, so their scheduled file checks fire.
///
/// ### Arguments
/// - `visual_cx`: The visual test context driving the window
pub(crate) fn settle_file_checks(visual_cx: &mut VisualTestContext) {
    advance_file_watch_clock(visual_cx, FILE_WATCH_DEBOUNCE);
}
