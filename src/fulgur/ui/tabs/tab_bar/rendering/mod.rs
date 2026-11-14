mod drag_ghost;
mod tab;

use super::{TabBar, TabBarEvent, tab_bar_button_factory};
use crate::fulgur::ui::{
    icons::CustomIcon, tabs::tab_drag::DraggedTab, window_drag::window_drag_region,
};
use gpui_kit::component::ActiveTheme;
use gpui_kit::{
    AnyElement, ClickEvent, Context, DragMoveEvent, InteractiveElement, IntoElement, ParentElement,
    Render, Role, StatefulInteractiveElement, Styled, Window, div, px,
};

impl Render for TabBar {
    /// Render the tab bar from the owning window's current tab list
    ///
    /// ### Arguments
    /// - `window`: The window to render the tab bar in
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered tab bar element
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

        let Some(fulgur_entity) = self.fulgur.upgrade() else {
            return div().into_any_element();
        };
        self.process_pending_scroll(&fulgur_entity, cx);
        let ghost = self.resolve_drag_ghost(&fulgur_entity, cx);
        let ghost_opacity = Self::sample_ghost_opacity(ghost.is_some(), window, cx);
        let fulgur = fulgur_entity.read(cx);
        let in_title_bar = fulgur.settings.app_settings.uses_unified_title_bar();
        let mut bar = div()
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .bg(cx.theme().tab_bar);
        if in_title_bar {
            bar = bar.flex_1().min_w(px(0.));
        }
        bar.child(
            tab_bar_button_factory("new-tab", "New Tab", CustomIcon::Plus, cx.theme().border)
                .on_click(cx.listener(|_, _, _window, cx| {
                    cx.emit(TabBarEvent::NewTab);
                })),
        )
        .child(
            tab_bar_button_factory(
                "open-file",
                "Open File (+Shift - Open Path)",
                CustomIcon::FolderOpen,
                cx.theme().border,
            )
            .on_click(cx.listener(|_, event: &ClickEvent, _window, cx| {
                if event.modifiers().shift {
                    cx.emit(TabBarEvent::OpenPath);
                } else {
                    cx.emit(TabBarEvent::OpenFile);
                }
            })),
        )
        .child(
            tab_bar_button_factory(
                "save-file",
                "Save File (+Shift - Save As)",
                CustomIcon::Save,
                cx.theme().border,
            )
            .border_r_1()
            .on_click(cx.listener(|_, event: &ClickEvent, _window, cx| {
                if event.modifiers().shift {
                    cx.emit(TabBarEvent::SaveFileAs);
                } else {
                    cx.emit(TabBarEvent::SaveFile);
                }
            })),
        )
        .child(
            div()
                .id("tab-scroll-container")
                .role(Role::TabList)
                .overflow_x_scroll()
                .track_scroll(&self.scroll_handle)
                .flex()
                .flex_1()
                .items_center()
                .children(self.render_tabs_with_slots(fulgur, ghost.as_ref(), ghost_opacity, cx))
                .child(Self::render_trailing_strip(in_title_bar, cx)),
        )
        .into_any_element()
    }
}

impl TabBar {
    /// Render the empty strip right of the last tab.
    ///
    /// ### Arguments
    /// - `in_title_bar`: Whether the tab bar is embedded in the unified title bar
    /// - `cx`: The tab bar context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered trailing strip
    fn render_trailing_strip(in_title_bar: bool, cx: &Context<Self>) -> AnyElement {
        use crate::fulgur::ui::components_utils::TAB_BAR_HEIGHT;

        let strip = div()
            .id("tab-bar-trailing")
            .flex_1()
            .min_w(px(0.))
            .border_b_1()
            .border_color(cx.theme().border)
            .h(TAB_BAR_HEIGHT)
            .on_drag_move::<DraggedTab>(cx.listener(
                |this, event: &DragMoveEvent<DraggedTab>, _window, cx| {
                    let cursor = event.event.position;
                    let bounds = event.bounds;
                    if cursor.x < bounds.origin.x
                        || cursor.x > bounds.origin.x + bounds.size.width
                        || cursor.y < bounds.origin.y
                        || cursor.y > bounds.origin.y + bounds.size.height
                    {
                        return;
                    }
                    let Some(fulgur) = this.fulgur.upgrade() else {
                        return;
                    };
                    let slot = fulgur.read(cx).tabs.len();
                    let dragged = event.drag(cx).clone();
                    this.drag_ghost = Some((slot, dragged));
                    cx.notify();
                },
            ))
            .on_drop(cx.listener(|this, dragged: &DraggedTab, _window, cx| {
                if let Some((slot, _)) = this.drag_ghost.take() {
                    cx.emit(TabBarEvent::Drop {
                        dragged: dragged.clone(),
                        slot,
                    });
                }
            }));
        if in_title_bar {
            return window_drag_region(strip, cx).into_any_element();
        }
        strip
            .on_click(cx.listener(|_, event: &ClickEvent, _window, cx| {
                if event.click_count() == 2 {
                    cx.emit(TabBarEvent::NewTab);
                }
            }))
            .into_any_element()
    }
}
