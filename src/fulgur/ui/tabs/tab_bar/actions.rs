use super::{
    CloseAllOtherTabs, CloseAllTabsAction, CloseTabAction, CloseTabsToLeft, CloseTabsToRight,
    DuplicateTab, ShowInFileManager,
};
use crate::fulgur::Fulgur;
use gpui::{Context, Window};
use gpui_component::{ActiveTheme, Theme, ThemeRegistry};

impl Fulgur {
    /// Handle close tab action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_tab_action(
        &mut self,
        action: &CloseTabAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tab(action.0, window, cx);
    }

    /// Handle close tabs to left action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_tabs_to_left(
        &mut self,
        action: &CloseTabsToLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tabs_to_left(action.0, window, cx);
    }

    /// Handle close tabs to right action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_tabs_to_right(
        &mut self,
        action: &CloseTabsToRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tabs_to_right(action.0, window, cx);
    }

    /// Handle close all tabs action from context menu
    ///
    /// ### Arguments
    /// - `_`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_all_tabs_action(
        &mut self,
        _: &CloseAllTabsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_all_tabs(window, cx);
    }

    /// Handle close all other tabs action from context menu
    ///
    /// ### Arguments
    /// - `_`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_all_other_tabs_action(
        &mut self,
        _: &CloseAllOtherTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_other_tabs(window, cx);
    }

    /// Handle show in file manager action from context menu.
    ///
    /// Opens the file manager and selects the file associated with the given tab.
    ///
    /// On macOS, uses `open -R` to reveal and select the file in Finder.
    /// On Windows, uses `explorer /select,` to select the file in Explorer.
    /// On Linux, falls back to opening the parent directory, as there is no
    /// universal "reveal file" command across desktop environments.
    ///
    /// ### Arguments
    /// - `action`: The action carrying the tab index
    /// - `_window`: The window context
    /// - `_cx`: The application context
    pub fn on_show_in_file_manager(
        &mut self,
        action: &ShowInFileManager,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(action.0) else {
            return;
        };
        let Some(editor_tab) = tab.as_editor() else {
            return;
        };
        let Some(file_path) = editor_tab.file_path() else {
            return;
        };

        let result = reveal_file_in_file_manager(file_path);
        if let Err(e) = result {
            log::error!("Failed to open file manager: {}", e);
        }
    }

    /// Handle duplicate tab action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action carrying the tab index
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_duplicate_tab(
        &mut self,
        action: &DuplicateTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate_tab(action.0, window, cx);
    }

    /// Handle next tab action
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_next_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_index) = self.active_tab_index {
            let next_index = (active_index + 1) % self.tabs.len();
            self.set_active_tab(next_index, window, cx);
        }
    }

    /// Handle previous tab action
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_previous_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_index) = self.active_tab_index {
            let previous_index = (active_index + self.tabs.len() - 1) % self.tabs.len();
            self.set_active_tab(previous_index, window, cx);
        }
    }

    /// Handle theme switching action.
    ///
    /// Applies the selected theme, updates settings, refreshes windows, and rebuilds menus.
    ///
    /// ### Arguments
    /// - `theme_name`: The name of the theme to switch to (as SharedString from action)
    /// - `cx`: The application context
    pub fn switch_to_theme(&mut self, theme_name: gpui::SharedString, cx: &mut Context<Self>) {
        if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(theme_name.as_ref())
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme_config);
            self.settings.app_settings.theme = theme_name;
            self.settings.app_settings.scrollbar_show = Some(cx.theme().scrollbar_show);
            let _ = self.update_and_propagate_settings(cx);
        }
        cx.refresh_windows();
        let menus =
            crate::fulgur::ui::menus::build_menus(self.settings.recent_files.get_files(), None);
        self.update_menus(menus, cx);
    }
}

/// Reveals a file in the platform's native file manager with the file selected.
///
/// - **macOS**: `open -R <path>` — reveals and selects the file in Finder
/// - **Windows**: `explorer /select,<path>` — selects the file in Explorer
/// - **Linux**: falls back to opening the parent directory via the `open` crate,
///   as there is no universal "reveal" command across desktop environments
///
/// ### Arguments
/// - `file_path`: The path of the file to reveal
///
/// ### Returns
/// - `Ok(())` on success, `Err` with an error message on failure
fn reveal_file_in_file_manager(file_path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(file_path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", file_path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let parent = file_path
            .parent()
            .ok_or_else(|| "File has no parent directory".to_string())?;
        open::that(parent).map_err(|e| e.to_string())
    }
}
