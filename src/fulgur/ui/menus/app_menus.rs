use super::actions::{
    About, CheckForUpdates, ClearRecentFiles, CloseAllFiles, CloseFile, CloseWindow, FindInFile,
    GetTheme, JumpToLine, NewFile, NewWindow, NextTab, NoneAction, OpenFile, OpenPath,
    OpenRecentFile, OpenRemote, PreviousTab, PrintFile, Quit, SaveFile, SaveFileAs, SelectTheme,
    SettingsTab, ToggleColorPicker, ToggleCommandPalette,
};
use crate::fulgur::Fulgur;
use crate::fulgur::shared_state::SharedAppState;
use gpui_kit::base::input::{AddCursorAbove, AddCursorBelow};
#[cfg(not(target_os = "macos"))]
use gpui_kit::component::GlobalState;
use gpui_kit::{App, Menu, MenuItem};
use std::path::PathBuf;

/// Build the menus for the Fulgur instance
///
/// ### Arguments
/// - `recent_files`: The list of recent files to display, most recent first
/// - `update_available`: Whether an update check found a newer release
///
/// ### Returns
/// - `Vec<Menu>`: The menus for the Fulgur instance
pub fn build_menus(recent_files: &[PathBuf], update_available: bool) -> Vec<Menu> {
    let recent_files_items = if recent_files.is_empty() {
        vec![MenuItem::action("No recent files", NoneAction)]
    } else {
        let mut items: Vec<MenuItem> = recent_files
            .iter()
            .map(|file| MenuItem::action(file.display().to_string(), OpenRecentFile(file.clone())))
            .collect();
        items.push(MenuItem::Separator);
        items.push(MenuItem::action("Clear recent files", ClearRecentFiles));
        items
    };
    vec![
        Menu {
            name: "Fulgur".into(),
            disabled: false,
            items: vec![
                MenuItem::action("About Fulgur", About),
                if update_available {
                    MenuItem::action("Update available", CheckForUpdates)
                } else {
                    MenuItem::action("Check for updates", CheckForUpdates)
                },
                MenuItem::Separator,
                MenuItem::action("Settings", SettingsTab),
                MenuItem::action("Select theme", SelectTheme),
                MenuItem::action("Get more themes...", GetTheme),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            disabled: false,
            items: vec![
                MenuItem::action("New", NewFile),
                MenuItem::action("New Window", NewWindow),
                MenuItem::action("Open...", OpenFile),
                MenuItem::action("Open from path...", OpenPath),
                MenuItem::action("Open remote file...", OpenRemote),
                MenuItem::Submenu(Menu {
                    name: "Recent Files".into(),
                    disabled: false,
                    items: recent_files_items,
                }),
                MenuItem::separator(),
                MenuItem::action("Save", SaveFile),
                MenuItem::action("Save as...", SaveFileAs),
                MenuItem::separator(),
                MenuItem::action("Print...", PrintFile),
                MenuItem::separator(),
                MenuItem::action("Close file", CloseFile),
                MenuItem::action("Close all files", CloseAllFiles),
                MenuItem::action("Close Window", CloseWindow),
            ],
        },
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Undo", gpui_kit::component::input::Undo),
                MenuItem::action("Redo", gpui_kit::component::input::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", gpui_kit::component::input::Cut),
                MenuItem::action("Copy", gpui_kit::component::input::Copy),
                MenuItem::action("Paste", gpui_kit::component::input::Paste),
                MenuItem::separator(),
                MenuItem::action("Add Cursor Above", AddCursorAbove),
                MenuItem::action("Add Cursor Below", AddCursorBelow),
                MenuItem::separator(),
                MenuItem::action("Find & Replace", FindInFile),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Command palette", ToggleCommandPalette),
                MenuItem::separator(),
                MenuItem::action("Color picker", ToggleColorPicker),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: "Go".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Next Tab", NextTab),
                MenuItem::action("Previous Tab", PreviousTab),
                MenuItem::Separator,
                MenuItem::action("Jump to line", JumpToLine),
            ],
        },
    ]
}

/// Inputs of the application menus, compared between rebuilds
#[derive(PartialEq, Eq)]
struct AppMenuInputs {
    /// Recent file paths, most recent first
    recent_files: Vec<PathBuf>,
    /// Whether an update check found a newer release
    update_available: bool,
}

impl AppMenuInputs {
    /// Read the menu inputs from the shared application state.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AppMenuInputs`: The current recent files and update state
    fn current(cx: &App) -> Self {
        let shared = cx.global::<SharedAppState>();
        Self {
            recent_files: shared.settings.get_recent_files(),
            update_available: shared.available_update.is_some(),
        }
    }
}

impl Fulgur {
    /// Register the app-scope observer that rebuilds the application menus when their inputs change.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn register_app_menus_observer(cx: &mut App) {
        let mut last_inputs = AppMenuInputs::current(cx);
        cx.observe_global::<SharedAppState>(move |cx| {
            let inputs = AppMenuInputs::current(cx);
            if last_inputs == inputs {
                return;
            }
            rebuild_app_menus(&inputs, cx);
            last_inputs = inputs;
        })
        .detach();
    }
}

/// Rebuild the application menus, and reload every window's `AppMenuBar` on non-macOS platforms.
///
/// ### Arguments
/// - `inputs`: The recent files and update state to show
/// - `cx`: The application context
fn rebuild_app_menus(inputs: &AppMenuInputs, cx: &mut App) {
    cx.set_menus(build_menus(&inputs.recent_files, inputs.update_available));
    #[cfg(not(target_os = "macos"))]
    {
        if let Some(owned_menus) = cx.get_menus() {
            GlobalState::global_mut(cx).set_app_menus(owned_menus);
        }
        let windows = cx
            .global::<crate::fulgur::window_manager::WindowManager>()
            .get_all_windows();
        for fulgur in windows.into_iter().filter_map(|weak| weak.upgrade()) {
            fulgur.update(cx, |this: &mut Fulgur, cx| {
                this.title_bar.update(
                    cx,
                    crate::fulgur::ui::bars::titlebar::CustomTitleBar::reload_app_menu_bar,
                );
            });
        }
    }
}

#[cfg(test)]
mod tests {
    use super::build_menus;
    use crate::fulgur::ui::menus::CloseWindow;
    use core::prelude::v1::test;
    use gpui_kit::base::input::{AddCursorAbove, AddCursorBelow};
    use gpui_kit::{Action, MenuItem};

    fn menu_exposes(menu_name: &str, action: &dyn Action) -> bool {
        build_menus(&[], false)
            .iter()
            .filter(|menu| menu.name == menu_name)
            .flat_map(|menu| menu.items.iter())
            .any(|item| match item {
                MenuItem::Action { action: bound, .. } => bound.partial_eq(action),
                _ => false,
            })
    }

    #[test]
    fn test_edit_menu_exposes_multi_cursor_actions() {
        assert!(menu_exposes("Edit", &AddCursorAbove));
        assert!(menu_exposes("Edit", &AddCursorBelow));
    }

    #[test]
    fn test_close_window_lives_in_the_file_menu_only() {
        assert!(menu_exposes("File", &CloseWindow));
        assert!(!menu_exposes("Fulgur", &CloseWindow));
    }
}
