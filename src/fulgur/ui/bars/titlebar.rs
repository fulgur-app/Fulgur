// Custom title bar with platform-specific menu bar placement

use gpui::{
    App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString, Styled,
    Window, div,
};
#[cfg(not(target_os = "macos"))]
use gpui_component::menu::AppMenuBar;
use gpui_component::{ActiveTheme, StyledExt, TitleBar, h_flex};

const DEFAULT_TITLE: &str = "Fulgur";

pub struct CustomTitleBar {
    #[cfg(not(target_os = "macos"))]
    app_menu_bar: Entity<AppMenuBar>,
    tab_title: Option<SharedString>,
    window_name: Option<String>,
    title: SharedString,
}

impl CustomTitleBar {
    /// Create a new custom title bar
    ///
    /// ### Arguments
    /// - `_window`: The window to create the title bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Entity<CustomTitleBar>`: The new custom title bar
    pub fn new(_window: &mut Window, cx: &mut App) -> Entity<Self> {
        #[cfg(not(target_os = "macos"))]
        let app_menu_bar = AppMenuBar::new(cx);

        cx.new(|_cx| Self {
            #[cfg(not(target_os = "macos"))]
            app_menu_bar,
            tab_title: None,
            window_name: None,
            title: SharedString::new_static(DEFAULT_TITLE),
        })
    }

    /// Reload the app menu bar from the current `GlobalState` menus (non-macOS only)
    #[cfg(not(target_os = "macos"))]
    pub fn reload_app_menu_bar(&mut self, cx: &mut Context<Self>) {
        self.app_menu_bar
            .update(cx, gpui_component::menu::AppMenuBar::reload);
    }

    /// Check whether the displayed title was composed from the given inputs
    ///
    /// ### Arguments
    /// - `title`: The candidate file or tab title
    /// - `window_name`: The candidate window identifier
    ///
    /// ### Returns
    /// - `bool`: `true` if the displayed title already reflects both inputs
    pub fn title_matches(&self, title: Option<&str>, window_name: Option<&str>) -> bool {
        self.tab_title.as_deref() == title && self.window_name.as_deref() == window_name
    }

    /// Set the title of the title bar.
    ///
    /// When `window_name` is `Some`, appends the name in parentheses to disambiguate
    /// multiple open windows, e.g. `"foo.rs - Fulgur (A)"` or `"Fulgur (A)"`.
    ///
    /// ### Arguments
    /// - `title`: The file or tab title to display; `None` shows only the app name
    /// - `window_name`: The window identifier to append; `None` omits it
    pub fn set_title(&mut self, title: Option<SharedString>, window_name: Option<&str>) {
        if self.title_matches(title.as_deref(), window_name) {
            return;
        }
        let suffix = window_name.map(|n| format!(" ({n})")).unwrap_or_default();
        self.title = match &title {
            Some(t) => format!("{t} - Fulgur{suffix}").into(),
            None => format!("{DEFAULT_TITLE}{suffix}").into(),
        };
        self.tab_title = title;
        self.window_name = window_name.map(ToString::to_string);
    }
}

impl Render for CustomTitleBar {
    /// Render the custom title bar
    ///
    /// ### Arguments
    /// - `_window`: The window to render the title bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered custom title bar
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
            use gpui::Styled;

            title_bar = title_bar.child(div().w_20());
        }
        title_bar
    }
}
