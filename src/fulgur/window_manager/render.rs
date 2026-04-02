use super::WindowManager;
use crate::fulgur::{Fulgur, state_persistence};
use gpui::*;
use gpui_component::WindowExt;

impl Fulgur {
    /// Process window state updates during the render cycle:
    /// 1. Cache the current window bounds and display ID for state persistence
    /// 2. Update the global WindowManager to track this window as focused
    /// 3. Display any pending notifications that were queued during event processing
    ///
    /// ### Arguments
    /// - `window`: The window being rendered
    /// - `cx`: The application context
    pub fn process_window_state_updates(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let display_id = window.display(cx).map(|d| d.id().into());
        self.cached_window_bounds =
            Some(state_persistence::SerializedWindowBounds::from_gpui_bounds(
                window.window_bounds(),
                display_id,
            ));
        cx.update_global::<WindowManager, _>(|manager, _| {
            manager.set_focused(self.window_id);
        });
        if let Some((notification_type, message)) = self.pending_notification.take() {
            window.push_notification((notification_type, message), cx);
        }
        // Check for notifications from background sync operations
        let sync_notification = self
            .shared_state(cx)
            .sync_state
            .pending_notification
            .lock()
            .take();
        if let Some((notification_type, message)) = sync_notification {
            window.push_notification((notification_type, message), cx);
        }
        #[cfg(target_os = "macos")]
        self.update_dock_menu_if_changed(cx);
        #[cfg(target_os = "windows")]
        self.update_jump_list_if_changed(cx);
    }
}
