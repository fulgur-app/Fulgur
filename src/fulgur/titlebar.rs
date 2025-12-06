// Custom title bar with platform-specific menu bar placement
use gpui::*;
#[cfg(not(target_os = "macos"))]
use gpui_component::menu::AppMenuBar;
use gpui_component::{ActiveTheme, StyledExt, TitleBar, h_flex};

const DEFAULT_TITLE: &str = "Fulgur";

pub struct CustomTitleBar {
    #[cfg(not(target_os = "macos"))]
    app_menu_bar: Entity<AppMenuBar>,
    title: String,
}

impl CustomTitleBar {
    // Create a new custom title bar
    // @param window: The window to create the title bar in
    // @param cx: The application context
    // @return: The new custom title bar
    pub fn new(_window: &mut Window, _cx: &mut App) -> Entity<Self> {
        #[cfg(not(target_os = "macos"))]
        let app_menu_bar = AppMenuBar::new(_window, _cx);

        _cx.new(|_cx| Self {
            #[cfg(not(target_os = "macos"))]
            app_menu_bar,
            title: DEFAULT_TITLE.to_string(),
        })
    }

    // Set the title of the title bar
    // @param title: The title to set (if None, the default title is used)
    pub fn set_title(&mut self, title: Option<String>) {
        if let Some(title) = title {
            self.title = format!("{} - Fulgur", title);
        } else {
            self.title = DEFAULT_TITLE.to_string();
        }
    }
}

impl Render for CustomTitleBar {
    // Render the custom title bar
    // @param window: The window to render the title bar in
    // @param cx: The application context
    // @return: The rendered custom title bar
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut title_bar = TitleBar::new().bg(cx.theme().tab_bar);
        #[cfg(not(target_os = "macos"))]
        {
            title_bar =
                title_bar.child(div().flex().items_center().child(self.app_menu_bar.clone()));
        }
        title_bar = title_bar.child(
            h_flex().flex_1().justify_center().items_center().child(
                div()
                    .text_sm()
                    .font_semibold()
                    .text_color(cx.theme().foreground)
                    .child(self.title.clone()),
            ),
        );
        #[cfg(not(target_os = "macos"))]
        {
            title_bar = title_bar.child(div().w_40());
        }
        #[cfg(target_os = "macos")]
        {
            title_bar = title_bar.child(div().w_20());
        }
        title_bar
    }

}
