use gpui::*;
use gpui_component::ThemeRegistry;

actions!(
    lightspeed,
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
        SettingsTab
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = lightspeed, no_json)]
pub struct SwitchTheme(pub SharedString);

// Build the menus for the Lightspeed instance
// @param cx: The application context
// @return: The menus for the Lightspeed instance
pub fn build_menus(cx: &mut App) -> Vec<Menu> {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    vec![
        Menu {
            name: "Lightspeed".into(),
            items: vec![
                MenuItem::action("About Lightspeed", About),
                MenuItem::Separator,
                MenuItem::Submenu(Menu {
                    name: "Theme".into(),
                    items: themes
                        .iter()
                        .map(|theme| {
                            MenuItem::action(theme.name.clone(), SwitchTheme(theme.name.clone()))
                        })
                        .collect(),
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
