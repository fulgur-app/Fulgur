// Custom title bar with platform-specific menu bar placement
use gpui::*;
use gpui_component::{ActiveTheme, StyledExt, TitleBar};
#[cfg(target_os = "windows")]
use gpui_component::menu::AppMenuBar;

pub struct CustomTitleBar {
    #[cfg(target_os = "windows")]
    app_menu_bar: Entity<AppMenuBar>,
}

impl CustomTitleBar {
    // Create a new custom title bar
    // @param window: The window to create the title bar in
    // @param cx: The application context
    // @return: The new custom title bar
    pub fn new(_window: &mut Window, _cx: &mut App) -> Entity<Self> {
        #[cfg(target_os = "windows")]
        let app_menu_bar = AppMenuBar::new(_window, _cx);

        _cx.new(|_cx| Self {
            #[cfg(target_os = "windows")]
            app_menu_bar,
        })
    }
}

impl Render for CustomTitleBar {
    // Render the custom title bar
    // @param window: The window to render the title bar in
    // @param cx: The application context
    // @return: The rendered custom title bar
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
