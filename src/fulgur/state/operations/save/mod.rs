mod snapshot;
#[cfg(all(test, feature = "gpui-test-support"))]
mod tests;

use super::super::persistence::WindowsState;
use crate::fulgur::{Fulgur, editor_tab::FileMissingProbe};
use gpui_kit::{App, Window};

/// When a session snapshot is taken, which bounds the disk access it may do.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SnapshotKind {
    /// Taken while the user keeps working (debounced autosave, tab-level saves):
    /// it must never touch the disk, since one tab on an unreachable mount would
    /// freeze the editor.
    Background,
    /// Taken while the user waits on it (quit, window close): it may `stat` the
    /// files the watcher does not cover.
    Blocking,
}

impl Fulgur {
    /// Decide how a snapshot of this window detects a deleted local file.
    ///
    /// ### Arguments
    /// - `kind`: When the snapshot is taken
    ///
    /// ### Returns
    /// - `FileMissingProbe::Stat`: A blocking snapshot while files are not watched
    /// - `FileMissingProbe::Reported`: Otherwise, the watcher reports deletions
    pub(crate) fn file_missing_probe(&self, kind: SnapshotKind) -> FileMissingProbe {
        if kind == SnapshotKind::Blocking && !self.settings.editor_settings.watch_files {
            FileMissingProbe::Stat
        } else {
            FileMissingProbe::Reported
        }
    }

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
        let windows_state = self.collect_windows_state(cx, Some(window), SnapshotKind::Blocking);
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
        let windows_state = self.collect_windows_state(cx, None, SnapshotKind::Blocking);
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
        let Some(windows_state) = Self::build_session_snapshot(cx, SnapshotKind::Blocking) else {
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
    /// - `kind`: When the snapshot is taken
    ///
    /// ### Returns
    /// - `Some(WindowsState)`: The snapshot of the registered windows
    /// - `None`: No window is registered, so the persisted session must be kept
    pub(super) fn build_session_snapshot(cx: &mut App, kind: SnapshotKind) -> Option<WindowsState> {
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
                            entity.read(cx).build_window_state(cx, window, kind)
                        })
                        .ok()
                });
            let snapshot = with_live_bounds
                .unwrap_or_else(|| entity.read(cx).build_window_state_without_bounds(cx, kind));
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
        let windows_state = self.collect_windows_state(cx, Some(window), SnapshotKind::Background);
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        shared.state_writer.save_async(windows_state);
    }

    /// Assemble a multi-window state snapshot, optionally leaving this window out.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    /// - `current_window`: This window, needed for its bounds; `None` omits it
    /// - `kind`: When the snapshot is taken
    ///
    /// ### Returns
    /// - `WindowsState`: The snapshot of the registered windows
    fn collect_windows_state(
        &self,
        cx: &App,
        current_window: Option<&Window>,
        kind: SnapshotKind,
    ) -> WindowsState {
        let window_manager = cx.global::<crate::fulgur::window_manager::WindowManager>();
        let mut windows_state = WindowsState { windows: vec![] };
        let current_window_id = self.window_id;
        let all_window_ids = window_manager.get_all_window_ids();
        for window_id in &all_window_ids {
            if *window_id == current_window_id {
                if let Some(window) = current_window {
                    windows_state
                        .windows
                        .push(self.build_window_state(cx, window, kind));
                }
            } else if let Some(weak_entity) = window_manager.get_window(*window_id)
                && let Some(entity) = weak_entity.upgrade()
            {
                windows_state
                    .windows
                    .push(entity.read(cx).build_window_state_without_bounds(cx, kind));
            }
        }
        windows_state
    }
}
