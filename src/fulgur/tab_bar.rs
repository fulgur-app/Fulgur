use crate::fulgur::{
    Fulgur,
    components_utils::{self, TAB_BAR_HEIGHT},
    icons::CustomIcon,
    tab::Tab,
};
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    context_menu::ContextMenuExt,
};
use serde::Deserialize;

// Define actions for tab context menu
#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabAction(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabsToLeft(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabsToRight(pub usize);

actions!(fulgur, [CloseAllTabsAction]);

// Create a tab bar button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button
// @param icon: The icon of the button
// @param border_color: The color of the border
// @return: A tab bar button
pub fn tab_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    let mut button = components_utils::button_factory(id, tooltip, icon, border_color);
    button = button.border_b_1();
    button
}

impl Fulgur {
    // Get the display title for a tab, including parent folder if there are duplicates
    // @param index: The index of the tab
    // @param tab: The tab to get the title for
    // @return: A tuple of (filename, optional parent folder)
    fn get_tab_display_title(&self, index: usize, tab: &Tab) -> (String, Option<String>) {
        let base_title = tab.title();

        // Only apply folder logic to editor tabs with file paths
        if let Some(editor_tab) = tab.as_editor() {
            if let Some(ref path) = editor_tab.file_path {
                let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

                // Count how many tabs have the same filename
                let duplicate_count = self
                    .tabs
                    .iter()
                    .enumerate()
                    .filter(|(i, t)| {
                        if *i == index {
                            return false; // Skip the current tab
                        }
                        if let Some(editor) = t.as_editor() {
                            if let Some(ref other_path) = editor.file_path {
                                if let Some(other_filename) =
                                    other_path.file_name().and_then(|n| n.to_str())
                                {
                                    return other_filename == filename;
                                }
                            }
                        }
                        false
                    })
                    .count();

                // If there are duplicates, include the parent folder
                if duplicate_count > 0 {
                    if let Some(parent) = path.parent() {
                        if let Some(parent_name) = parent.file_name().and_then(|n| n.to_str()) {
                            return (filename.to_string(), Some(format!("../{}", parent_name)));
                        }
                    }
                }

                return (filename.to_string(), None);
            }
        }

        (base_title.to_string(), None)
    }

    // Handle close tab action from context menu
    pub(super) fn on_close_tab_action(
        &mut self,
        action: &CloseTabAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tab(action.0, window, cx);
    }

    // Handle close tabs to left action from context menu
    pub(super) fn on_close_tabs_to_left(
        &mut self,
        action: &CloseTabsToLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tabs_to_left(action.0, window, cx);
    }

    // Handle close tabs to right action from context menu
    pub(super) fn on_close_tabs_to_right(
        &mut self,
        action: &CloseTabsToRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tabs_to_right(action.0, window, cx);
    }

    // Handle close all tabs action from context menu
    pub(super) fn on_close_all_tabs_action(
        &mut self,
        _: &CloseAllTabsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_all_tabs(window, cx);
    }

    // Render the tab bar
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered tab bar element
    pub(super) fn render_tab_bar(&self, window: &mut Window, cx: &mut Context<Self>) -> Div {
        div()
            .flex()
            .items_center()
            .h(px(TAB_BAR_HEIGHT))
            .bg(cx.theme().tab_bar)
            // Do not delete this, it is used to create a space for the title bar
            .child(
                div()
                    .w_20()
                    .h(px(TAB_BAR_HEIGHT))
                    .border_b_1()
                    .border_color(cx.theme().border),
            )
            .child(
                tab_bar_button_factory("new-tab", "New Tab", CustomIcon::Plus, cx.theme().border)
                    .on_click(cx.listener(|this, _, window, cx| {
                        this.new_tab(window, cx);
                    })),
            )
            .child(
                tab_bar_button_factory(
                    "open-file",
                    "Open File",
                    CustomIcon::FolderOpen,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.open_file(window, cx);
                })),
            )
            .child(
                div()
                    .id("tab-scroll-container")
                    .overflow_x_scroll()
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
                            .h(px(TAB_BAR_HEIGHT)),
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
    ) -> AnyElement {
        let tab_id = tab.id();
        let is_active = match self.active_tab_index {
            Some(active_index) => index == active_index,
            None => false,
        };

        let has_tabs_on_left = index > 0;
        let has_tabs_on_right = index < self.tabs.len() - 1;
        let total_tabs = self.tabs.len();

        let mut tab_div = div()
            .id(("tab", tab_id))
            .flex()
            .items_center()
            .h(px(TAB_BAR_HEIGHT))
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

        let (filename, folder) = self.get_tab_display_title(index, tab);
        let modified_indicator = if tab.is_modified() { " •" } else { "" };

        let mut title_container = div().flex().items_center().gap_1().pl_1().child(
            div()
                .text_sm()
                .text_color(if is_active {
                    cx.theme().tab_active_foreground
                } else {
                    cx.theme().tab_foreground
                })
                .child(format!("{}{}", filename, modified_indicator)),
        );

        if let Some(folder_path) = folder {
            title_container = title_container.child(
                div()
                    .text_xs()
                    .italic()
                    .text_color(if is_active {
                        cx.theme().tab_active_foreground
                    } else {
                        cx.theme().tab_foreground
                    })
                    .child(folder_path),
            );
        }

        tab_div
            .child(title_container)
            .child(
                Button::new(("close-tab", tab_id))
                    .icon(CustomIcon::Close)
                    .ghost()
                    .xsmall()
                    .cursor_pointer()
                    .on_click(cx.listener(move |this, _, window, cx| {
                        cx.stop_propagation();
                        this.close_tab(tab_id, window, cx);
                    })),
            )
            .context_menu(move |this, _window, _cx| {
                this.menu("Close Tab", Box::new(CloseTabAction(tab_id)))
                    .menu_with_disabled(
                        "Close Tabs to the Left",
                        Box::new(CloseTabsToLeft(index)),
                        !has_tabs_on_left,
                    )
                    .menu_with_disabled(
                        "Close Tabs to the Right",
                        Box::new(CloseTabsToRight(index)),
                        !has_tabs_on_right,
                    )
                    .separator()
                    .menu_with_disabled(
                        "Close All Tabs",
                        Box::new(CloseAllTabsAction),
                        total_tabs == 0,
                    )
            })
            .into_any_element()
    }
}
