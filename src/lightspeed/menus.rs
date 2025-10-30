use gpui::*;
use gpui_component::ThemeRegistry;


actions!(lightspeed, [About, Quit, CloseWindow, NewFile, OpenFile, SaveFileAs, SaveFile, CloseFile, CloseAllFiles]);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = lightspeed, no_json)]
pub struct SwitchTheme(pub SharedString);

pub fn build_menus(cx: &mut App) -> Vec<Menu> {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    vec![
        Menu {
            name: "Lightspeed".into(),
            items: vec![
                MenuItem::Submenu(Menu {
                    name: "Theme".into(),
                    items: themes
                        .iter()
                        .map(|theme| MenuItem::action(theme.name.clone(), SwitchTheme(theme.name.clone())))
                        .collect(),
                }),
                MenuItem::action("About Lightspeed", About),
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
                MenuItem::action("Save as...", SaveFileAs),
                MenuItem::action("Save", SaveFile),
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
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![MenuItem::action("Close Window", CloseWindow)],
        },
    ]
}