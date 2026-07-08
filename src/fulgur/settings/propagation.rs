use crate::fulgur::{Fulgur, shared_state::SharedAppState, ui::tabs::tab::Tab};
use gpui::{BorrowAppContext, Context, Window};
use gpui_component::notification::NotificationType;

impl Fulgur {
    /// Update settings and propagate to all windows
    ///
    /// This method should be called whenever settings are changed. It will:
    /// 1. Save settings to disk
    /// 2. Publish the settings to `SharedAppState` via `cx.update_global`
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

        // Save settings to disk
        if let Err(e) = self.settings.save() {
            log::error!("Failed to save settings: {e}");
            self.pending_notification = Some((
                NotificationType::Error,
                format!("Failed to save settings: {e}").into(),
            ));
            return Err(e);
        }

        // Mark settings as changed for this window so the next render pushes
        // the new editor settings into this window's tabs.
        self.settings_changed = true;

        let settings = self.settings.clone();
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.settings = settings;
        });

        Ok(())
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
