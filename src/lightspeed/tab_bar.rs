use crate::lightspeed::{Lightspeed, components_utils, tab::Tab};
use gpui::*;
use gpui_component::{
    ActiveTheme, IconName, Sizable,
    button::{Button, ButtonVariants},
};

// Create a tab bar button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button
// @param icon: The icon of the button
// @param border_color: The color of the border
// @return: A tab bar button
pub fn tab_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: IconName,
    border_color: Hsla,
) -> Button {
    let mut button = components_utils::button_factory(id, tooltip, icon, border_color);
    button = button.border_b_1();
    button
}

impl Lightspeed {
    // Render the tab bar
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered tab bar element
    pub(super) fn render_tab_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .h(px(40.))
            .bg(cx.theme().tab_bar)
            .child(
                tab_bar_button_factory("new-tab", "New Tab", IconName::Plus, cx.theme().border)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.new_tab(window, cx);
                    })),
            )
            .child(
                tab_bar_button_factory(
                    "open-file",
                    "Open File",
                    IconName::FolderOpen,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_file(window, cx);
                })),
            )
            .child(
                div()
                    .flex()
                    .flex_1()
                    .items_center()
                    .children(
                        self.tabs
                            .iter()
                            .enumerate()
                            .map(|(index, tab)| self.render_tab(index, tab, window, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .border_b_1()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .h(px(40.)),
                    ),
            )
    }

    // Render a single tab
    // @param index: The index of the tab
    // @param tab: The tab to render
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered tab element
    fn render_tab(
        &self,
        index: usize,
        tab: &Tab,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Div {
        let tab_id = tab.id();
        let is_active = match self.active_tab_index {
            Some(active_index) => index == active_index,
            None => false,
        };

        let mut tab_div = div()
            .flex()
            .items_center()
            .h(px(40.))
            .px_2()
            .gap_2()
            .border_l_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx| {
                    if !is_active {
                        this.set_active_tab(index, window, cx);
                    }
                }),
            );

        if is_active {
            tab_div = tab_div.bg(cx.theme().tab_active).border_b_0();
        } else {
            tab_div = tab_div
                .bg(cx.theme().tab)
                .hover(|this| this.bg(cx.theme().muted))
                .cursor_pointer();
        }

        tab_div
            .child(
                div()
                    .text_sm()
                    .text_color(if is_active {
                        cx.theme().tab_active_foreground
                    } else {
                        cx.theme().tab_foreground
                    })
                    .pl_1()
                    .child(format!(
                        "{}{}",
                        tab.title(),
                        if tab.is_modified() { " •" } else { "" }
                    )),
            )
            .child(
                Button::new(("close-tab", tab_id))
                    .icon(IconName::Close)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_tab(tab_id, window, cx);
                    })),
            )
    }
}
