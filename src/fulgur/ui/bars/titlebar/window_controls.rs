// The minimize, maximize and close buttons of the unified layout on Windows and Linux

use super::CustomTitleBar;
use crate::fulgur::ui::icons::CustomIcon;
use gpui::{
    AnyElement, App, Context, Hsla, InteractiveElement, IntoElement, ParentElement,
    StatefulInteractiveElement, Styled, Window, div,
};
use gpui_component::{ActiveTheme, Sizable, TITLE_BAR_HEIGHT, h_flex};

/// Render the window control cluster sitting at the end of the unified row
///
/// ### Arguments
/// - `window`: The window being rendered, queried for the controls it supports
/// - `cx`: The title bar context
///
/// ### Returns
/// - `AnyElement`: The rendered cluster, empty when the window manager supports nothing
pub(super) fn render_window_controls(
    window: &mut Window,
    cx: &mut Context<CustomTitleBar>,
) -> AnyElement {
    // A tiling compositor may honor neither minimize nor maximize; close is always ours.
    let supported = window.window_controls();
    let mut controls = h_flex()
        .id("unified-window-controls")
        .flex_shrink_0()
        .h_full()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border);
    if supported.minimize {
        controls = controls.child(render_control(Control::Minimize, cx));
    }
    if supported.maximize {
        let control = if window.is_maximized() {
            Control::Restore
        } else {
            Control::Maximize
        };
        controls = controls.child(render_control(control, cx));
    }
    controls
        .child(render_control(Control::Close, cx))
        .into_any_element()
}

/// One of the window control buttons
#[derive(Clone, Copy)]
enum Control {
    Minimize,
    Maximize,
    Restore,
    Close,
}

impl Control {
    /// The stable element identifier of this button
    ///
    /// ### Returns
    /// - `&'static str`: The element id
    fn id(self) -> &'static str {
        match self {
            Self::Minimize => "window-minimize",
            Self::Maximize => "window-maximize",
            Self::Restore => "window-restore",
            Self::Close => "window-close",
        }
    }

    /// The icon drawn inside this button
    ///
    /// ### Returns
    /// - `CustomIcon`: The icon of the button
    fn icon(self) -> CustomIcon {
        match self {
            Self::Minimize => CustomIcon::WindowMinimize,
            Self::Maximize => CustomIcon::WindowMaximize,
            Self::Restore => CustomIcon::WindowRestore,
            Self::Close => CustomIcon::WindowClose,
        }
    }

    /// The area the platform window has to associate with this button
    ///
    /// ### Returns
    /// - `WindowControlArea`: The matching control area
    fn area(self) -> gpui::WindowControlArea {
        match self {
            Self::Minimize => gpui::WindowControlArea::Min,
            Self::Maximize | Self::Restore => gpui::WindowControlArea::Max,
            Self::Close => gpui::WindowControlArea::Close,
        }
    }

    /// The hover and pressed colors of this button
    ///
    /// ### Arguments
    /// - `cx`: The application context, read for the active theme
    ///
    /// ### Returns
    /// - `(Hsla, Hsla, Hsla)`: The hovered background, the pressed background and the text color
    fn colors(self, cx: &App) -> (Hsla, Hsla, Hsla) {
        if matches!(self, Self::Close) {
            (
                cx.theme().danger,
                cx.theme().danger_active,
                cx.theme().danger_foreground,
            )
        } else {
            (
                cx.theme().secondary_hover,
                cx.theme().secondary_active,
                cx.theme().secondary_foreground,
            )
        }
    }
}

/// Render a single window control button
///
/// ### Arguments
/// - `control`: The button to render
/// - `cx`: The title bar context
///
/// ### Returns
/// - `AnyElement`: The rendered button
fn render_control(control: Control, cx: &mut Context<CustomTitleBar>) -> AnyElement {
    let (hovered_bg, pressed_bg, emphasis) = control.colors(cx);
    let button = div()
        .id(control.id())
        .flex()
        .w(TITLE_BAR_HEIGHT)
        .h_full()
        .flex_shrink_0()
        .justify_center()
        .items_center()
        .text_color(cx.theme().foreground)
        .hover(|style| style.bg(hovered_bg).text_color(emphasis))
        .active(|style| style.bg(pressed_bg).text_color(emphasis))
        .child(control.icon().icon().small());
    #[cfg(target_os = "windows")]
    {
        button
            .window_control_area(control.area())
            .into_any_element()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = control.area();
        button
            .on_mouse_down(gpui::MouseButton::Left, |_, window, cx| {
                window.prevent_default();
                cx.stop_propagation();
            })
            .on_click(move |_, window, cx| {
                cx.stop_propagation();
                match control {
                    Control::Minimize => window.minimize_window(),
                    Control::Maximize | Control::Restore => window.zoom_window(),
                    Control::Close => window.remove_window(),
                }
            })
            .into_any_element()
    }
}
