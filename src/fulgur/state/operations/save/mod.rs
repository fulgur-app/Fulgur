mod snapshot;
#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

use super::super::persistence::WindowsState;
use crate::fulgur::Fulgur;
use gpui_kit::{App, Window};

impl Fulgur {
    /// Save the current app state to disk (saves all windows in multi-window mode)
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    ///
    /// ### Errors
    /// - Returns an error if the state cannot be persisted (no state database
    ///   available, or the transaction failed).
    ///
    /// ### Returns
    /// - `Ok(())`: If the app state was saved successfully
    /// - `Err(anyhow::Error)`: If the app state could not be saved
    pub fn save_state(&self, cx: &App, window: &Window) -> anyhow::Result<()> {
        log::debug!("Saving application state...");
        let windows_state = self.build_windows_state(cx, window);
        let window_count = windows_state.windows.len();
        let tab_count = self.tabs.len();
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_blocking(windows_state)?;
        log::debug!(
            "Application state saved successfully ({window_count} windows, {tab_count} tabs in this window)"
        );
        Ok(())
    }

    /// Save the state of every other window, dropping this one from the database.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// - Returns an error if the state cannot be persisted (no state database
    ///   available, or the transaction failed).
    ///
    /// ### Returns
    /// - `Ok(())`: If the remaining windows were saved successfully
    /// - `Err(anyhow::Error)`: If the state could not be saved
    pub fn save_state_without_this_window(&self, cx: &App) -> anyhow::Result<()> {
        log::debug!(
            "Saving application state without window {:?}...",
            self.window_id
        );
        let windows_state = self.collect_windows_state(cx, None);
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_blocking(windows_state)?;
        Ok(())
    }

    /// Persist the session and any pending settings write when the application quits, whatever triggered the quit.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn register_app_quit_state_save(cx: &mut App) {
        cx.on_app_quit(|cx| {
            Self::save_state_on_app_quit(cx);
            cx.global::<crate::fulgur::shared_state::SharedAppState>()
                .settings_writer
                .flush();
            async {}
        })
        .detach();
    }

    /// Snapshot every registered window and write the session synchronously.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    fn save_state_on_app_quit(cx: &mut App) {
        let Some(windows_state) = Self::build_session_snapshot(cx) else {
            log::debug!("No window registered at quit, keeping the persisted session");
            return;
        };
        let window_count = windows_state.windows.len();
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        match shared.state_writer.save_blocking(windows_state) {
            Ok(()) => log::info!("Application state saved on quit ({window_count} windows)"),
            Err(e) => log::error!("Failed to save app state on quit: {e}"),
        }
    }

    /// Snapshot every registered window without needing one of them in scope.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(WindowsState)`: The snapshot of the registered windows
    /// - `None`: No window is registered, so the persisted session must be kept
    pub(super) fn build_session_snapshot(cx: &mut App) -> Option<WindowsState> {
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let entities: Vec<_> = window_manager
            .get_all_window_ids()
            .into_iter()
            .filter_map(|window_id| {
                let entity = window_manager.get_window(window_id)?.upgrade()?;
                Some((window_id, entity))
            })
            .collect();
        if entities.is_empty() {
            return None;
        }
        let handles = cx.windows();
        let mut windows_state = WindowsState { windows: vec![] };
        for (window_id, entity) in entities {
            let with_live_bounds = handles
                .iter()
                .find(|handle| handle.window_id() == window_id)
                .and_then(|handle| {
                    handle
                        .update(cx, |_, window, cx| {
                            entity.read(cx).build_window_state(cx, window)
                        })
                        .ok()
                });
            let snapshot = with_live_bounds
                .unwrap_or_else(|| entity.read(cx).build_window_state_without_bounds(cx));
            windows_state.windows.push(snapshot);
        }
        Some(windows_state)
    }

    /// Save the current app state to disk without blocking the UI thread.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The window to save (needed for window bounds)
    pub fn save_state_async(&self, cx: &App, window: &Window) {
        log::debug!("Saving application state (async)...");
        let windows_state = self.build_windows_state(cx, window);
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_async(windows_state);
    }

    /// Assemble the full multi-window state snapshot for persistence.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `window`: The current window (needed for its bounds)
    ///
    /// ### Returns
    /// - `WindowsState`: The snapshot of all open windows
    fn build_windows_state(&self, cx: &App, window: &Window) -> WindowsState {
        self.collect_windows_state(cx, Some(window))
    }

    /// Assemble a multi-window state snapshot, optionally leaving this window out.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `current_window`: This window, needed for its bounds; `None` omits it
    ///
    /// ### Returns
    /// - `WindowsState`: The snapshot of the registered windows
    fn collect_windows_state(&self, cx: &App, current_window: Option<&Window>) -> WindowsState {
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        let current_window_id = self.window_id;
        let all_window_ids = window_manager.get_all_window_ids();
        for window_id in &all_window_ids {
            if *window_id == current_window_id {
                if let Some(window) = current_window {
                    windows_state
                        .windows
                        .push(self.build_window_state(cx, window));
                }
            } else if let Some(weak_entity) = window_manager.get_window(*window_id)
                && let Some(entity) = weak_entity.upgrade()
            {
                windows_state
                    .windows
                    .push(entity.read(cx).build_window_state_without_bounds(cx));
            }
        }
        windows_state
    }
}
