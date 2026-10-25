use crate::fulgur::shared_state::AppNotification;
use crate::fulgur::{Fulgur, shared_state::SharedAppState};
use gpui_kit::component::notification::NotificationType;
use gpui_kit::{BorrowAppContext, Context, Window};
use std::path::PathBuf;

impl Fulgur {
    /// Update settings and propagate to all windows
    ///
    /// This method should be called whenever settings are changed. It will:
    /// 1. Save settings to disk
    /// 2. Publish the settings to `SharedAppState` via `cx.update_global`
    ///
    /// Every window (including this one) observes the global with
    /// `observe_global_in` and applies the new editor settings to its tabs
    /// from that observer.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Errors
    /// Returns an error if persisting the settings to disk fails. The shared
    /// state update and re-render still happen even when saving fails.
    ///
    /// ### Returns
    /// - `anyhow::Result<()>`: Result of the operation
    pub fn update_and_propagate_settings(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        // Apply log level immediately so the change takes effect in this session.
        crate::fulgur::utils::logger::set_debug_mode(self.settings.app_settings.debug_mode);

        if let Err(e) = Fulgur::shared_state(cx)
            .settings_writer
            .save_blocking(&self.settings)
        {
            log::error!("Failed to save settings: {e}");
            Fulgur::shared_state(cx).notify(AppNotification::foreground(
                NotificationType::Error,
                format!("Failed to save settings: {e}"),
            ));
            return Err(e);
        }

        let settings = self.settings.clone();
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.settings = settings;
        });

        Ok(())
    }

    /// Move a file to the top of the shared recent files and persist them in the background
    ///
    /// ### Arguments
    /// - `file`: The opened file path, or the URL of a remote file
    /// - `cx`: The application context
    pub fn record_recent_file(&mut self, file: PathBuf, cx: &mut Context<Self>) {
        let recent_files = cx.update_global::<SharedAppState, _>(|shared, cx| {
            shared.settings.add_recent_file(file);
            shared
                .settings_writer
                .save_in_background(shared.settings.clone(), cx.background_executor());
            shared.settings.recent_files.clone()
        });
        self.settings.recent_files = recent_files;
    }

    /// Apply the current editor settings to every tab in this window
    ///
    /// ### Arguments
    /// - `window`: The window containing the tabs
    /// - `cx`: The application context
    pub fn apply_editor_settings_to_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let settings = self.settings.editor_settings.clone();
        for tab in self.tabs.clone() {
            tab.update(cx, |tab, cx| {
                tab.update_settings(window, cx, &settings);
            });
        }
    }
}
