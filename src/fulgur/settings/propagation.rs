use crate::fulgur::{Fulgur, ui::tabs::tab::Tab, window_manager};
use gpui::{Context, Window};
use gpui_component::notification::NotificationType;

impl Fulgur {
    /// Update settings and propagate to all windows
    ///
    /// This method should be called whenever settings are changed. It will:
    /// 1. Save settings to disk
    /// 2. Update shared settings in SharedAppState
    /// 3. Increment the shared settings version (so other windows detect the change)
    /// 4. Set settings_changed flag for this window
    /// 5. Force all windows to re-render immediately
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `anyhow::Result<()>`: Result of the operation
    pub fn update_and_propagate_settings(&mut self, cx: &mut Context<Self>) -> anyhow::Result<()> {
        // Apply log level immediately so the change takes effect in this session.
        crate::fulgur::utils::logger::set_debug_mode(self.settings.app_settings.debug_mode);

        // Save settings to disk
        if let Err(e) = self.settings.save() {
            log::error!("Failed to save settings: {}", e);
            self.pending_notification = Some((
                NotificationType::Error,
                format!("Failed to save settings: {}", e).into(),
            ));
            return Err(e);
        }

        // Update shared settings
        let shared = self.shared_state(cx);
        *shared.settings.lock() = self.settings.clone();

        // Increment the version counter so other windows detect the change
        let new_version = shared
            .settings_version
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst)
            + 1;
        self.local_settings_version = new_version;

        // Mark settings as changed for this window
        self.settings_changed = true;

        log::debug!(
            "Window {:?} updated settings to version {}, notifying other windows",
            self.window_id,
            new_version
        );

        // Force other windows to re-render immediately
        // (Skip the current window to avoid reentrancy issues - it will re-render naturally)
        let current_window_id = self.window_id;
        let window_manager = cx.global::<window_manager::WindowManager>();
        let all_windows = window_manager.get_all_windows();

        // Defer notifications to avoid reentrancy issues
        cx.defer(move |cx| {
            for weak_window in all_windows.iter() {
                if let Some(window_entity) = weak_window.upgrade() {
                    // Skip the current window (already updating)
                    let should_notify = window_entity.read(cx).window_id != current_window_id;
                    if should_notify {
                        window_entity.update(cx, |_, cx| {
                            cx.notify();
                        });
                    }
                }
            }
        });

        Ok(())
    }

    /// Synchronize settings from other windows
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn synchronize_settings_from_other_windows(&mut self, cx: &mut Context<Self>) {
        let shared = self.shared_state(cx);
        let shared_version = shared
            .settings_version
            .load(std::sync::atomic::Ordering::Relaxed);
        if shared_version > self.local_settings_version {
            // Settings have been updated in another window - reload them
            let shared_settings = shared.settings.lock().clone();
            self.settings = shared_settings;
            self.local_settings_version = shared_version;
            self.settings_changed = true;
            log::debug!(
                "Window {:?} detected settings change from another window (version {} -> {})",
                self.window_id,
                self.local_settings_version,
                shared_version
            );
        }
    }

    /// Propagate settings changes to tabs
    ///
    /// ### Arguments
    /// - `window`: The window containing the tabs
    /// - `cx`: The application context
    pub fn propagate_settings_to_tabs(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.tabs_pending_update.is_empty() {
            let settings = self.settings.editor_settings.clone();
            for tab_index in self.tabs_pending_update.drain() {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
                    editor_tab.update_settings(window, cx, &settings);
                }
            }
        }
        if self.settings_changed {
            let settings = self.settings.editor_settings.clone();
            for tab_index in self.rendered_tabs.iter().copied().collect::<Vec<_>>() {
                if let Some(Tab::Editor(editor_tab)) = self.tabs.get_mut(tab_index) {
                    editor_tab.update_settings(window, cx, &settings);
                }
            }
            self.settings_changed = false;
        }
    }

    /// Track newly rendered tabs and mark them for settings update
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn track_newly_rendered_tabs(&mut self, cx: &mut Context<Self>) {
        if let Some(index) = self.active_tab_index {
            let is_newly_rendered = !self.rendered_tabs.contains(&index);
            self.rendered_tabs.insert(index);
            if is_newly_rendered {
                self.tabs_pending_update.insert(index);
                cx.notify();
            }
        }
    }
}
