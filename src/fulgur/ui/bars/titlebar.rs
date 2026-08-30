// Custom title bar with platform-specific menu bar placement

use crate::fulgur::Fulgur;
use gpui::{
    AnyElement, App, AppContext, Context, Entity, IntoElement, ParentElement, Render, SharedString,
    Styled, WeakEntity, Window, div,
};
#[cfg(not(target_os = "macos"))]
use gpui_component::menu::AppMenuBar;
use gpui_component::{ActiveTheme, StyledExt, TitleBar, h_flex};

const DEFAULT_TITLE: &str = "Fulgur";

pub struct CustomTitleBar {
    #[cfg(not(target_os = "macos"))]
    app_menu_bar: Entity<AppMenuBar>,
    /// Read at render time for the settings and the tab bar of the owning window.
    /// Only the unified macOS layout needs it.
    #[cfg(target_os = "macos")]
    fulgur: WeakEntity<Fulgur>,
    tab_title: Option<SharedString>,
    window_name: Option<String>,
    title: SharedString,
    #[cfg(target_os = "macos")]
    should_move_window: bool,
}

impl CustomTitleBar {
    /// Create a new custom title bar
    ///
    /// ### Arguments
    /// - `fulgur`: Weak handle to the owning window, read for settings and the tab bar
    /// - `_window`: The window to create the title bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Entity<CustomTitleBar>`: The new custom title bar
    pub fn new(fulgur: &WeakEntity<Fulgur>, _window: &mut Window, cx: &mut App) -> Entity<Self> {
        #[cfg(not(target_os = "macos"))]
        let app_menu_bar = AppMenuBar::new(cx);
        #[cfg(not(target_os = "macos"))]
        let _ = fulgur;

        cx.new(|_cx| Self {
            #[cfg(not(target_os = "macos"))]
            app_menu_bar,
            #[cfg(target_os = "macos")]
            fulgur: fulgur.clone(),
            tab_title: None,
            window_name: None,
            title: SharedString::new_static(DEFAULT_TITLE),
            #[cfg(target_os = "macos")]
            should_move_window: false,
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

    /// Render the classic layout: a title row above a separate tab bar row
    ///
    /// ### Arguments
    /// - `cx`: The title bar context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered title row
    fn render_classic(&self, cx: &mut Context<Self>) -> AnyElement {
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
        title_bar.into_any_element()
    }
}

#[cfg(target_os = "macos")]
mod unified {
    use super::CustomTitleBar;
    use crate::fulgur::window_manager::WindowManager;
    use gpui::{
        AnyElement, App, Context, InteractiveElement, IntoElement, MouseButton, ParentElement,
        Pixels, Styled, Window, div, px,
    };
    use gpui_component::{ActiveTheme, InteractiveElementExt, StyledExt, TITLE_BAR_HEIGHT, h_flex};

    /// Horizontal room reserved on the left for the macOS traffic light buttons
    const TRAFFIC_LIGHTS_WIDTH: Pixels = px(80.0);
    /// Left padding used in fullscreen, where the traffic lights are hidden
    const FULLSCREEN_LEFT_PADDING: Pixels = px(12.0);
    /// Blank gap kept between the window badge and the first tab, about one character wide
    const BADGE_GAP: Pixels = px(10.0);

    impl CustomTitleBar {
        /// Check whether the owning window asked for the unified title bar layout
        ///
        /// ### Arguments
        /// - `cx`: The title bar context
        ///
        /// ### Returns
        /// - `bool`: `true` when the title bar should embed the tab bar
        pub(super) fn uses_unified_layout(&self, cx: &App) -> bool {
            self.fulgur.upgrade().is_some_and(|fulgur| {
                fulgur
                    .read(cx)
                    .settings
                    .app_settings
                    .uses_unified_title_bar()
            })
        }

        /// Render the unified layout: window controls, window badge and tab bar on one row
        ///
        /// ### Description
        /// Only the badge slot carries the window move handlers; the tab bar keeps its own
        /// mouse handling so tab clicks and tab drag and drop are unaffected. The empty
        /// space right of the last tab is turned into a second move region by the tab bar
        /// itself.
        ///
        /// ### Arguments
        /// - `window`: The window being rendered, queried for its fullscreen state
        /// - `cx`: The title bar context
        ///
        /// ### Returns
        /// - `AnyElement`: The rendered unified row, or an empty element if the window is gone
        pub(super) fn render_unified(
            &mut self,
            window: &mut Window,
            cx: &mut Context<Self>,
        ) -> AnyElement {
            let Some(fulgur_entity) = self.fulgur.upgrade() else {
                return div().into_any_element();
            };
            let (tab_bar, window_id) = {
                let fulgur = fulgur_entity.read(cx);
                (fulgur.tab_bar.clone(), fulgur.window_id)
            };
            let window_name = cx
                .global::<WindowManager>()
                .get_window_name(window_id)
                .map(str::to_owned);
            let left_padding = if window.is_fullscreen() {
                FULLSCREEN_LEFT_PADDING
            } else {
                TRAFFIC_LIGHTS_WIDTH
            };
            div()
                .flex_shrink_0()
                .child(
                    h_flex()
                        .id("unified-title-bar")
                        .w_full()
                        .h(TITLE_BAR_HEIGHT)
                        .items_center()
                        .bg(cx.theme().tab_bar)
                        .child(Self::render_window_badge_slot(
                            left_padding,
                            window_name.as_deref(),
                            cx,
                        ))
                        .child(tab_bar),
                )
                .into_any_element()
        }

        /// Render the draggable slot between the traffic lights and the first tab
        ///
        /// ### Arguments
        /// - `left_padding`: Room reserved on the left for the traffic lights
        /// - `window_name`: The window identifier to show as a badge, if the app named this window
        /// - `cx`: The title bar context
        ///
        /// ### Returns
        /// - `AnyElement`: The rendered slot, wired for window move and double click to zoom
        fn render_window_badge_slot(
            left_padding: Pixels,
            window_name: Option<&str>,
            cx: &mut Context<Self>,
        ) -> AnyElement {
            let badge = window_name.map(|name| {
                div()
                    .px(px(6.0))
                    .py(px(2.0))
                    .rounded_sm()
                    .bg(cx.theme().muted)
                    .text_xs()
                    .font_semibold()
                    .text_color(cx.theme().muted_foreground)
                    .child(name.to_owned())
            });
            h_flex()
                .id("title-bar-window-slot")
                .flex_shrink_0()
                .h_full()
                .items_center()
                .pl(left_padding)
                .pr(BADGE_GAP)
                .border_b_1()
                .border_color(cx.theme().border)
                .on_double_click(|_, window, _| window.titlebar_double_click())
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(|this, _, _window, _cx| {
                        this.should_move_window = true;
                    }),
                )
                .on_mouse_up(
                    MouseButton::Left,
                    cx.listener(|this, _, _window, _cx| {
                        this.should_move_window = false;
                    }),
                )
                .on_mouse_down_out(cx.listener(|this, _, _window, _cx| {
                    this.should_move_window = false;
                }))
                .on_mouse_move(cx.listener(|this, _, window, _cx| {
                    if this.should_move_window {
                        this.should_move_window = false;
                        window.start_window_move();
                    }
                }))
                .children(badge)
                .into_any_element()
        }
    }
}

impl Render for CustomTitleBar {
    /// Render the custom title bar
    ///
    /// ### Arguments
    /// - `window`: The window to render the title bar in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered custom title bar
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        #[cfg(target_os = "macos")]
        if self.uses_unified_layout(cx) {
            return self.render_unified(window, cx);
        }
        #[cfg(not(target_os = "macos"))]
        let _ = window;
        self.render_classic(cx)
    }
}
