use gpui::*;
use gpui_component::{dropdown::*, *};
#[cfg(target_os = "windows")]
use gpui_component::menu::AppMenuBar;
mod themes;

// Define actions for the app menus
actions!(lightspeed, [About, Quit, CloseWindow]);

fn init_menus(cx: &mut App) {
    cx.set_menus(vec![
        Menu {
            name: "Lightspeed".into(),
            items: vec![
                MenuItem::action("About Lightspeed", About),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
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
    ]);
}

// Custom title bar with platform-specific menu bar placement
pub struct CustomTitleBar {
    #[cfg(target_os = "windows")]
    app_menu_bar: Entity<AppMenuBar>,
}

impl CustomTitleBar {
    fn new(_window: &mut Window, _cx: &mut App) -> Entity<Self> {
        #[cfg(target_os = "windows")]
        let app_menu_bar = AppMenuBar::new(_window, _cx);

        _cx.new(|_cx| Self {
            #[cfg(target_os = "windows")]
            app_menu_bar,
        })
    }
}

impl Render for CustomTitleBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut title_bar = TitleBar::new();

        // Left side - menu bar on Windows only
        #[cfg(target_os = "windows")]
        {
            title_bar = title_bar.child(
                div()
                    .flex()
                    .items_center()
                    .child(self.app_menu_bar.clone()),
            );
        }
        #[cfg(not(target_os = "windows"))]
        {
            title_bar = title_bar.child(div());
        }

        title_bar
            // Center - app title (using absolute positioning to center)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Lightspeed"),
                    ),
            )
            // Right side - empty for now, window controls are automatically added by TitleBar
            .child(div())
    }
}

pub struct HelloWorld {
    themes_dropdown: Entity<DropdownState<Vec<SharedString>>>,
    title_bar: Entity<CustomTitleBar>,
}

impl HelloWorld {
    fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let themes = ThemeRegistry::global(cx)
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .collect::<Vec<SharedString>>();
        
        let themes_dropdown = cx.new(|cx| {
            DropdownState::new(themes, Some(IndexPath::default()), window, cx)
        });

        let title_bar = CustomTitleBar::new(window, cx);

        cx.new(|cx| {
            cx.subscribe_in(&themes_dropdown, window, Self::on_theme_change)
                .detach();

            Self {
                themes_dropdown,
                title_bar,
            }
        })
    }

    fn on_theme_change(
        &mut self,
        _dropdown: &Entity<DropdownState<Vec<SharedString>>>,
        event: &DropdownEvent<Vec<SharedString>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let DropdownEvent::Confirm(Some(theme_name)) = event {
            if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
                Theme::global_mut(cx).apply_config(&theme_config);
                cx.refresh_windows();
            }
        }
    }
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .size_full()
            .v_flex()
            .child(self.title_bar.clone())
            .child(
                div()
                    .flex_1()
                    .v_flex()
                    .gap_2()
                    .items_center()
                    .justify_center()
                    .child("Hello, World!")
                    .child(div().w_128().child(
                        Dropdown::new(&self.themes_dropdown)
                            .placeholder("Select theme...")
                            .title_prefix("Theme: ")
                            .cleanable()
                            .into_any_element()
                    )),
            )
    }
}

fn main() {
    let app = Application::new();

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        themes::init(cx);
        
        // Initialize menus
        init_menus(cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                // Enable custom title bar
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                window.set_window_title("Lightspeed");
                let view = HelloWorld::new(window, cx);
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view.into(), window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}