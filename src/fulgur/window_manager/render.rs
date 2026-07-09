use super::WindowManager;
use crate::fulgur::{Fulgur, state};
use gpui::{BorrowAppContext, Context, Window};

impl Fulgur {
    /// Process window state updates during the render cycle:
    /// 1. Cache the current window bounds and display ID for state persistence
    /// 2. Update the global `WindowManager` to track this window as focused, but
    ///    only when it is the OS-active window (renders also fire for background
    ///    windows, so render order is not a reliable focus signal)
    ///
    /// ### Arguments
    /// - `window`: The window being rendered
    /// - `cx`: The application context
    pub fn process_window_state_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let display_id = window
            .display(cx)
            .and_then(|d| u32::try_from(u64::from(d.id())).ok());
        self.cached_window_bounds = Some(state::SerializedWindowBounds::from_gpui_bounds(
            window.window_bounds(),
            display_id,
        ));
        if window.is_window_active() {
            cx.update_global::<WindowManager, _>(|manager, _| {
                manager.set_focused(self.window_id);
            });
        }
        #[cfg(any(target_os = "macos", target_os = "windows"))]
        self.publish_window_menu_fingerprint_if_changed(cx);
        #[cfg(target_os = "macos")]
        self.update_dock_menu_if_changed(cx);
        #[cfg(target_os = "windows")]
        self.update_jump_list_if_changed(cx);
    }
}
