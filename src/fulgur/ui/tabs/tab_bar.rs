use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::components_utils::{self, TAB_BAR_BUTTON_SIZE, TAB_BAR_HEIGHT, button_factory},
    ui::icons::CustomIcon,
};
use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, StyledExt, Theme, ThemeRegistry,
    button::{Button, ButtonVariants},
    h_flex,
    menu::ContextMenuExt,
    tooltip::Tooltip,
    v_flex,
};
use serde::Deserialize;

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabAction(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabsToLeft(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseTabsToRight(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct CloseAllOtherTabs(pub usize);

actions!(fulgur, [CloseAllTabsAction]);

/// Create a tab bar button
///
/// ### Arguments
/// - `id`: The ID of the button
/// - `tooltip`: The tooltip of the button
/// - `icon`: The icon of the button
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Button`: A tab bar button
pub fn tab_bar_button_factory(
    id: &'static str,
    tooltip: &'static str,
    icon: CustomIcon,
    border_color: Hsla,
) -> Button {
    button_factory(id, tooltip, icon, border_color)
        .border_b_1()
        .h(TAB_BAR_BUTTON_SIZE)
        .w(TAB_BAR_BUTTON_SIZE)
}

impl Fulgur {
    /// Get the display title for a tab, including parent folder if there are duplicates
    ///
    /// ### Arguments
    /// - `index`: The index of the tab
    /// - `tab`: The tab to get the title for
    ///
    /// ### Returns
    /// - `(String, Option<String>)`: A tuple of (filename, optional parent folder)
    fn get_tab_display_title(&self, index: usize, tab: &Tab) -> (String, Option<String>) {
        let base_title = tab.title();
        if let Some(editor_tab) = tab.as_editor()
            && let Some(ref path) = editor_tab.file_path
        {
            let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");
            let duplicate_count = self
                .tabs
                .iter()
                .enumerate()
                .filter(|(i, t)| {
                    if *i == index {
                        return false;
                    }
                    if let Some(editor) = t.as_editor()
                        && let Some(ref other_path) = editor.file_path
                        && let Some(other_filename) =
                            other_path.file_name().and_then(|n| n.to_str())
                    {
                        return other_filename == filename;
                    }
                    false
                })
                .count();
            if duplicate_count > 0
                && let Some(parent) = path.parent()
                && let Some(parent_name) = parent.file_name().and_then(|n| n.to_str())
            {
                return (filename.to_string(), Some(format!("../{}", parent_name)));
            }

            return (filename.to_string(), None);
        }
        (base_title.to_string(), None)
    }

    /// Handle close tab action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_tab_action(
        &mut self,
        action: &CloseTabAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tab(action.0, window, cx);
    }

    /// Handle close tabs to left action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_tabs_to_left(
        &mut self,
        action: &CloseTabsToLeft,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tabs_to_left(action.0, window, cx);
    }

    /// Handle close tabs to right action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_tabs_to_right(
        &mut self,
        action: &CloseTabsToRight,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_tabs_to_right(action.0, window, cx);
    }

    /// Handle close all tabs action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_all_tabs_action(
        &mut self,
        _: &CloseAllTabsAction,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_all_tabs(window, cx);
    }

    /// Handle close all tabs action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action to handle
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_close_all_other_tabs_action(
        &mut self,
        _: &CloseAllOtherTabs,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.close_other_tabs(window, cx);
    }

    /// Handle next tab action
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_next_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_index) = self.active_tab_index {
            let next_index = (active_index + 1) % self.tabs.len();
            self.set_active_tab(next_index, window, cx);
        }
    }

    /// Handle previous tab action
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_previous_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(active_index) = self.active_tab_index {
            let previous_index = (active_index + self.tabs.len() - 1) % self.tabs.len();
            self.set_active_tab(previous_index, window, cx);
        }
    }

    /// Handle theme switching action
    ///
    /// Applies the selected theme, updates settings, refreshes windows, and rebuilds menus.
    ///
    /// ### Arguments
    /// - `theme_name`: The name of the theme to switch to (as SharedString from action)
    /// - `cx`: The application context
    pub fn switch_to_theme(&mut self, theme_name: gpui::SharedString, cx: &mut Context<Self>) {
        if let Some(theme_config) = ThemeRegistry::global(cx)
            .themes()
            .get(theme_name.as_ref())
            .cloned()
        {
            Theme::global_mut(cx).apply_config(&theme_config);
            self.settings.app_settings.theme = theme_name;
            self.settings.app_settings.scrollbar_show = Some(cx.theme().scrollbar_show);
            let _ = self.update_and_propagate_settings(cx);
        }
        cx.refresh_windows();
        let menus =
            crate::fulgur::ui::menus::build_menus(self.settings.recent_files.get_files(), None);
        cx.set_menus(menus);
    }

    /// Render the tab bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Div`: The rendered tab bar element
    pub fn render_tab_bar(&self, cx: &mut Context<Self>) -> Div {
        let mut tab_bar = div()
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .bg(cx.theme().tab_bar);
        tab_bar = tab_bar
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
                tab_bar_button_factory(
                    "save-file",
                    "Save File",
                    CustomIcon::Save,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, _, window, cx| {
                    this.save_file(window, cx);
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
                            .map(|(index, tab)| self.render_tab(index, tab, cx)),
                    )
                    .child(
                        div()
                            .flex_1()
                            .min_w(px(0.))
                            .border_b_1()
                            .border_l_1()
                            .border_color(cx.theme().border)
                            .h(TAB_BAR_HEIGHT),
                    ),
            );
        tab_bar
    }

    /// Render a single tab
    ///
    /// ### Arguments
    /// - `index`: The index of the tab
    /// - `tab`: The tab to render
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered tab element
    pub fn render_tab(&self, index: usize, tab: &Tab, cx: &mut Context<Self>) -> AnyElement {
        let tab_id = tab.id();
        let is_active = match self.active_tab_index {
            Some(active_index) => index == active_index,
            None => false,
        };
        let has_tabs_on_left = index > 0;
        let has_tabs_on_right = index < self.tabs.len() - 1;
        let total_tabs = self.tabs.len();
        let file_path = tab.as_editor().and_then(|editor_tab| {
            editor_tab
                .file_path
                .as_ref()
                .and_then(|path| path.to_str().map(|s| s.to_string()))
        });
        let mut tab_div = div()
            .id(("tab", tab_id))
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_l_1()
            .border_b_1()
            .border_color(cx.theme().border)
            .on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _, window, cx: &mut Context<'_, Fulgur>| {
                    if !is_active {
                        this.set_active_tab(index, window, cx);
                    }
                }),
            );
        if is_active {
            tab_div = tab_div.bg(cx.theme().tab_active).border_b_0();
            self.set_title(Some(tab.title().to_string()), cx);
        } else {
            tab_div = tab_div
                .bg(cx.theme().tab)
                .hover(|this| this.bg(cx.theme().muted))
                .cursor_pointer();
        }
        if let Some(path) = file_path {
            tab_div = tab_div.tooltip(move |window, cx| {
                let path_clone = path.clone();
                let file_info = std::fs::metadata(&path).ok();
                let file_size = file_info.as_ref().map(|meta| {
                    let size = meta.len();
                    if size < 1024 {
                        format!("{} B", size)
                    } else if size < 1024 * 1024 {
                        format!("{:.1} KB", size as f64 / 1024.0)
                    } else {
                        format!("{:.1} MB", size as f64 / (1024.0 * 1024.0))
                    }
                });
                let last_modified = file_info.as_ref().and_then(|meta| {
                    meta.modified()
                        .ok()
                        .map(components_utils::format_system_time)
                });
                let last_modified = last_modified.unwrap_or_default();
                Tooltip::element(move |_, cx| {
                    let mut tooltip = v_flex().gap_1().py_2().px_1().child(
                        h_flex()
                            .gap_3()
                            .child(CustomIcon::File.icon())
                            .child(path_clone.clone())
                            .text_sm()
                            .font_semibold(),
                    );
                    let mut details = h_flex().gap_4().justify_between();
                    if let Some(ref size) = file_size {
                        details = details.child(
                            div()
                                .child(format!("Size: {}", size))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        );
                    }
                    if let Some(ref last_modified) = last_modified {
                        details = details.child(
                            div()
                                .child(format!("Last Modified: {}", last_modified))
                                .text_xs()
                                .text_color(cx.theme().muted_foreground),
                        );
                    }
                    tooltip = tooltip.child(details);
                    tooltip
                })
                .build(window, cx)
            });
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
        let tab_with_content = tab_div
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
            .on_mouse_down(
                MouseButton::Middle,
                cx.listener(move |this, _, window, cx| {
                    this.close_tab(tab_id, window, cx);
                }),
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
                    .menu_with_disabled(
                        "Close All Other Tabs",
                        Box::new(CloseAllOtherTabs(index)),
                        total_tabs == 0,
                    )
            });

        tab_with_content.into_any_element()
    }
}

/// Opens the theme repository in the default browser
///
/// This is a standalone helper function for the GetTheme action.
pub fn open_theme_repository() {
    if let Err(e) = open::that("https://github.com/longbridge/gpui-component/tree/main/themes") {
        log::error!("Failed to open browser: {}", e);
    }
}
