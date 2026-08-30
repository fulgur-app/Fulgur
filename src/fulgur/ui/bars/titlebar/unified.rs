// The unified layout: window controls, window badge and tab bar sharing a single row

use super::CustomTitleBar;
use crate::fulgur::{ui::window_drag::window_drag_region, window_manager::WindowManager};
use gpui::{
    AnyElement, App, Context, InteractiveElement, IntoElement, ParentElement, Pixels, Styled,
    Window, div, px,
};
use gpui_component::{ActiveTheme, StyledExt, TITLE_BAR_HEIGHT, h_flex};

/// Horizontal room reserved on the left for the macOS traffic light buttons
#[cfg(target_os = "macos")]
const TRAFFIC_LIGHTS_WIDTH: Pixels = px(80.0);
/// Left padding used on macOS in fullscreen, where the traffic lights are hidden
#[cfg(target_os = "macos")]
const FULLSCREEN_LEFT_PADDING: Pixels = px(12.0);
/// Blank gap kept on either side of the window badge, about one character wide
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
        let mut row = h_flex()
            .id("unified-title-bar")
            .w_full()
            .h(TITLE_BAR_HEIGHT)
            .items_center()
            .bg(cx.theme().tab_bar)
            // Windows and Linux open on the burger menu, with no room to reserve first.
            .children(Self::render_leading_slot(Self::left_padding(window), cx));
        // macOS keeps the application menus in the system menu bar and draws the window
        // buttons itself, so both only exist in the row on Windows and Linux. The burger
        // sits right against the tab bar buttons, reading as one group of four.
        #[cfg(not(target_os = "macos"))]
        {
            row = row.child(super::burger_menu::render_burger_menu(&self.fulgur, cx));
        }
        row = row
            .child(tab_bar)
            .child(Self::render_window_badge_slot(window_name.as_deref(), cx));
        #[cfg(not(target_os = "macos"))]
        {
            row = row.child(super::window_controls::render_window_controls(window, cx));
        }
        div().flex_shrink_0().child(row).into_any_element()
    }

    /// Render the draggable slot reserving the room the window buttons need on the left
    ///
    /// ### Arguments
    /// - `width`: Room to reserve, the traffic light width on macOS
    /// - `cx`: The title bar context
    ///
    /// ### Returns
    /// - `Some(AnyElement)`: The rendered slot, wired for window move and double click to zoom
    /// - `None`: If nothing has to be reserved, as on Windows and Linux
    fn render_leading_slot(width: Pixels, cx: &mut Context<Self>) -> Option<AnyElement> {
        if width <= px(0.0) {
            return None;
        }
        let slot = div()
            .id("title-bar-leading-slot")
            .flex_shrink_0()
            .w(width)
            .h_full()
            .border_b_1()
            .border_color(cx.theme().border);
        Some(window_drag_region(slot, cx).into_any_element())
    }

    /// Compute the room to leave before the first element of the row
    ///
    /// ### Arguments
    /// - `window`: The window being rendered, queried for its fullscreen state
    ///
    /// ### Returns
    /// - `Pixels`: The traffic light width on macOS, nothing on Windows and Linux, where
    ///   the window buttons live at the other end of the row
    fn left_padding(window: &Window) -> Pixels {
        #[cfg(target_os = "macos")]
        {
            if window.is_fullscreen() {
                FULLSCREEN_LEFT_PADDING
            } else {
                TRAFFIC_LIGHTS_WIDTH
            }
        }
        #[cfg(not(target_os = "macos"))]
        {
            let _ = window;
            px(0.0)
        }
    }

    /// Render the draggable slot holding the window badge, at the end of the row
    ///
    /// ### Arguments
    /// - `window_name`: The window identifier to show as a badge, if the app named this window
    /// - `cx`: The title bar context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered slot, wired for window move and double click to zoom
    fn render_window_badge_slot(window_name: Option<&str>, cx: &mut Context<Self>) -> AnyElement {
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
        let mut slot = h_flex()
            .id("title-bar-window-slot")
            .flex_shrink_0()
            .h_full()
            .items_center();
        slot = if badge.is_some() {
            slot.px(BADGE_GAP)
        } else {
            slot.w(BADGE_GAP)
        };
        let slot = slot
            .border_b_1()
            .border_color(cx.theme().border)
            .children(badge);
        window_drag_region(slot, cx).into_any_element()
    }
}
