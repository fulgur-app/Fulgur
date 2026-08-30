// Turning parts of the unified title bar into window move regions

use gpui::{Context, Div, InteractiveElement, MouseButton, Render, Stateful};
#[cfg(not(target_os = "windows"))]
use gpui_component::InteractiveElementExt;

/// A view that owns the "a left button press started on a drag region" latch.
pub(crate) trait WindowDragState {
    /// Access the latch storing whether a window move may still start from this press
    ///
    /// ### Returns
    /// - `&mut bool`: The latch, `true` while a press may still turn into a window move
    fn window_drag_armed(&mut self) -> &mut bool;
}

/// Make an element behave like a native title bar: drag to move, double click to zoom.
///
/// ### Arguments
/// - `element`: The region to turn into a title bar drag handle
/// - `cx`: The context of the view owning the drag latch
///
/// ### Returns
/// - `Stateful<Div>`: The same element with the platform's move and zoom handling attached
pub(crate) fn window_drag_region<T>(element: Stateful<Div>, cx: &Context<T>) -> Stateful<Div>
where
    T: WindowDragState + Render,
{
    #[cfg(target_os = "windows")]
    let element = element.window_control_area(gpui::WindowControlArea::Drag);
    #[cfg(target_os = "macos")]
    let element = element.on_double_click(|_, window, _| window.titlebar_double_click());
    #[cfg(target_os = "linux")]
    let element = element
        .on_double_click(|_, window, _| window.zoom_window())
        .on_mouse_down(MouseButton::Right, |event, window, _| {
            window.show_window_menu(event.position);
        });
    element
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _, _window, _cx| {
                *this.window_drag_armed() = true;
            }),
        )
        .on_mouse_up(
            MouseButton::Left,
            cx.listener(|this, _, _window, _cx| {
                *this.window_drag_armed() = false;
            }),
        )
        .on_mouse_down_out(cx.listener(|this, _, _window, _cx| {
            *this.window_drag_armed() = false;
        }))
        .on_mouse_move(cx.listener(|this, _, window, _cx| {
            if *this.window_drag_armed() {
                *this.window_drag_armed() = false;
                window.start_window_move();
            }
        }))
}
