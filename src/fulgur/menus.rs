use gpui::*;
use gpui_component::{ThemeMode, ThemeRegistry};

actions!(
    fulgur,
    [
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
        AddTheme
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct SwitchTheme(pub SharedString);

// Build the menus for the Fulgur instance
// @param cx: The application context
// @return: The menus for the Fulgur instance
pub fn build_menus(cx: &mut App) -> Vec<Menu> {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    let light_themes = themes
        .iter()
        .filter(|theme| theme.mode == ThemeMode::Light)
        .collect::<Vec<_>>();
    let dark_themes = themes
        .iter()
        .filter(|theme| theme.mode == ThemeMode::Dark)
        .collect::<Vec<_>>();
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
                                    MenuItem::action(
                                        theme.name.clone(),
                                        SwitchTheme(theme.name.clone()),
                                    )
                                })
                                .collect(),
                        }),
                        MenuItem::Submenu(Menu {
                            name: "Dark Themes".into(),
                            items: dark_themes
                                .iter()
                                .map(|theme| {
                                    MenuItem::action(
                                        theme.name.clone(),
                                        SwitchTheme(theme.name.clone()),
                                    )
                                })
                                .collect(),
                        }),
                        MenuItem::Separator,
                        MenuItem::action("Get more themes", AddTheme),
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
            name: "Window".into(),
            items: vec![MenuItem::action("Close Window", CloseWindow)],
        },
    ]
}
