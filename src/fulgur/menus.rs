use gpui::*;
use gpui_component::{Theme, ThemeMode, ThemeRegistry};
use std::path::PathBuf;

actions!(
    fulgur,
    [
        NoneAction,
        About,
        Quit,
        CloseWindow,
        NewFile,
        OpenFile,
        SaveFileAs,
        SaveFile,
        CloseFile,
        CloseAllFiles,
        FindInFile,
        SettingsTab,
        GetTheme,
        AddTheme,
        NextTab,
        PreviousTab,
        JumpToLine,
        ClearRecentFiles,
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct OpenRecentFile(pub PathBuf);

// Build the menus for the Fulgur instance
// @param cx: The application context
// @param recent_files: The list of recent files to display
// @return: The menus for the Fulgur instance
pub fn build_menus(cx: &mut App, recent_files: &[PathBuf]) -> Vec<Menu> {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    let light_themes: Vec<String> = themes
        .iter()
        .filter(|theme| theme.mode == ThemeMode::Light)
        .map(|theme| theme.name.to_string())
        .collect();
    let dark_themes: Vec<String> = themes
        .iter()
        .filter(|theme| theme.mode == ThemeMode::Dark)
        .map(|theme| theme.name.to_string())
        .collect();
    let current_theme = Theme::global(cx).theme_name().to_string();
    let recent_files_items = if recent_files.is_empty() {
        let mut items = Vec::new();
        items.push(MenuItem::action("No recent files", NoneAction));
        items
    } else {
        let mut items: Vec<MenuItem> = recent_files
            .iter()
            .map(|file| {
                MenuItem::action(
                    file.display().to_string(),
                    OpenRecentFile(file.to_path_buf()),
                )
            })
            .collect();
        items.push(MenuItem::Separator);
        items.push(MenuItem::action("Clear recent files", ClearRecentFiles));
        items
    };
    vec![
        Menu {
            name: "Fulgur".into(),
            items: vec![
                MenuItem::action("About Fulgur", About),
                MenuItem::Separator,
                MenuItem::Submenu(Menu {
                    name: "Themes".into(),
                    items: vec![
                        MenuItem::Submenu(Menu {
                            name: "Light Themes".into(),
                            items: light_themes
                                .iter()
                                .map(|theme| {
                                    MenuItem::action(theme.clone(), SwitchTheme(theme.into()))
                                })
                                .collect(),
                        }),
                        MenuItem::Submenu(Menu {
                            name: "Dark Themes".into(),
                            items: dark_themes
                                .iter()
                                .map(|theme| {
                                    MenuItem::action(
                                        format!(
                                            "{} {}",
                                            theme,
                                            if theme == &current_theme {
                                                "\u{2713}"
                                            } else {
                                                ""
                                            }
                                        ),
                                        SwitchTheme(theme.into()),
                                    )
                                })
                                .collect(),
                        }),
                        MenuItem::Separator,
                        MenuItem::action("Get more themes...", GetTheme),
                        MenuItem::action("Add Theme...", AddTheme),
                    ],
                }),
                MenuItem::action("Settings", SettingsTab),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", NewFile),
                MenuItem::action("Open...", OpenFile),
                MenuItem::Submenu(Menu {
                    name: "Recent Files".into(),
                    items: recent_files_items,
                }),
                MenuItem::separator(),
                MenuItem::action("Save", SaveFile),
                MenuItem::action("Save as...", SaveFileAs),
                MenuItem::separator(),
                MenuItem::action("Close file", CloseFile),
                MenuItem::action("Close all files", CloseAllFiles),
            ],
        },
        Menu {
            name: "Edit".into(),
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
            name: "Go".into(),
            items: vec![
                MenuItem::action("Next Tab", NextTab),
                MenuItem::action("Previous Tab", PreviousTab),
                MenuItem::Separator,
                MenuItem::action("Jump to line", JumpToLine),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![MenuItem::action("Close Window", CloseWindow)],
        },
    ]
}
