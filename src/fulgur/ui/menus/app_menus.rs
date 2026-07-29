use super::actions::{
    About, CheckForUpdates, ClearRecentFiles, CloseAllFiles, CloseFile, CloseWindow, FindInFile,
    GetTheme, JumpToLine, NewFile, NewWindow, NextTab, NoneAction, OpenFile, OpenPath,
    OpenRecentFile, OpenRemote, PreviousTab, PrintFile, Quit, SaveFile, SaveFileAs, SelectTheme,
    SettingsTab, ToggleColorPicker,
};
use crate::fulgur::Fulgur;
use gpui::{Context, Menu, MenuItem};
#[cfg(not(target_os = "macos"))]
use gpui_component::GlobalState;
use std::path::PathBuf;

/// Build the menus for the Fulgur instance
///
/// ### Arguments
/// - `recent_files`: The list of recent files to display
/// - `update_link`: The optional link to the update
///
/// ### Returns
/// - `Vec<Menu>`: The menus for the Fulgur instance
pub fn build_menus(recent_files: &[PathBuf], update_link: Option<&str>) -> Vec<Menu> {
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
                if update_link.is_some() {
                    MenuItem::action("Update available", CheckForUpdates)
                } else {
                    MenuItem::action("Check for updates", CheckForUpdates)
                },
                MenuItem::Separator,
                MenuItem::action("Settings", SettingsTab),
                MenuItem::action("Select theme", SelectTheme),
                MenuItem::action("Get more themes...", GetTheme),
                MenuItem::Separator,
                MenuItem::action("Close Window", CloseWindow),
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
            ],
        },
        Menu {
            name: "Edit".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Undo", gpui_component::input::Undo),
                MenuItem::action("Redo", gpui_component::input::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", gpui_component::input::Cut),
                MenuItem::action("Copy", gpui_component::input::Copy),
                MenuItem::action("Paste", gpui_component::input::Paste),
                MenuItem::separator(),
                MenuItem::action("Find & Replace", FindInFile),
            ],
        },
        Menu {
            name: "View".into(),
            disabled: false,
            items: vec![
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

impl Fulgur {
    /// Set the application menus and sync them to the `AppMenuBar` on non-macOS platforms.
    ///
    /// ### Arguments
    /// - `menus`: The menus to set
    /// - `cx`: The application context
    pub fn update_menus(&mut self, menus: Vec<Menu>, cx: &mut Context<Self>) {
        cx.set_menus(menus);
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(owned_menus) = cx.get_menus() {
                GlobalState::global_mut(cx).set_app_menus(owned_menus);
            }
            self.title_bar.update(
                cx,
                crate::fulgur::ui::bars::titlebar::CustomTitleBar::reload_app_menu_bar,
            );
        }
    }
}
