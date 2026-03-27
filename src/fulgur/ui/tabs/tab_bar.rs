use crate::fulgur::{
    Fulgur,
    tab::Tab,
    ui::components_utils::{self, TAB_BAR_BUTTON_SIZE, TAB_BAR_HEIGHT, button_factory},
    ui::icons::CustomIcon,
    ui::tabs::tab_drag::DraggedTab,
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

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct ShowInFileManager(pub usize);

#[derive(Action, Clone, PartialEq, Deserialize)]
#[action(namespace = fulgur, no_json)]
pub struct DuplicateTab(pub usize);

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

    /// Handle show in file manager action from context menu
    ///
    /// Opens the file manager and selects the file associated with the given tab.
    ///
    /// On macOS, uses `open -R` to reveal and select the file in Finder.
    /// On Windows, uses `explorer /select,` to select the file in Explorer.
    /// On Linux, falls back to opening the parent directory, as there is no
    /// universal "reveal file" command across desktop environments.
    ///
    /// ### Arguments
    /// - `action`: The action carrying the tab index
    /// - `_window`: The window context
    /// - `_cx`: The application context
    pub fn on_show_in_file_manager(
        &mut self,
        action: &ShowInFileManager,
        _window: &mut Window,
        _cx: &mut Context<Self>,
    ) {
        let Some(tab) = self.tabs.get(action.0) else {
            return;
        };
        let Some(editor_tab) = tab.as_editor() else {
            return;
        };
        let Some(ref file_path) = editor_tab.file_path else {
            return;
        };

        let result = reveal_file_in_file_manager(file_path);
        if let Err(e) = result {
            log::error!("Failed to open file manager: {}", e);
        }
    }

    /// Handle duplicate tab action from context menu
    ///
    /// ### Arguments
    /// - `action`: The action carrying the tab index
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn on_duplicate_tab(
        &mut self,
        action: &DuplicateTab,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.duplicate_tab(action.0, window, cx);
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
        self.update_menus(menus, cx);
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
                    "Open File (+Shift - Open Path)",
                    CustomIcon::FolderOpen,
                    cx.theme().border,
                )
                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                    if event.modifiers().shift {
                        this.show_open_from_path_dialog(window, cx);
                    } else {
                        this.open_file(window, cx);
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
                .on_click(cx.listener(|this, event: &ClickEvent, window, cx| {
                    if event.modifiers().shift {
                        this.save_file_as(window, cx);
                    } else {
                        this.save_file(window, cx);
                    }
                })),
            )
            .child(
                div()
                    .id("tab-scroll-container")
                    .overflow_x_scroll()
                    .track_scroll(&self.tab_scroll_handle)
                    .flex()
                    .flex_1()
                    .items_center()
                    .children(self.render_tabs_with_slots(cx))
                    .child(
                        div()
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
                                    let slot = this.tabs.len();
                                    let dragged = event.drag(cx).clone();
                                    this.drag_ghost = Some((slot, dragged));
                                    cx.notify();
                                },
                            ))
                            .on_drop(cx.listener(|this, dragged: &DraggedTab, window, cx| {
                                if let Some((slot, _)) = this.drag_ghost.take() {
                                    this.handle_tab_drop(dragged, slot, window, cx);
                                }
                            })),
                    ),
            );
        tab_bar
    }

    /// Render a ghost tab shown at the insertion point during a drag operation.
    ///
    /// The ghost tab previews where the dragged tab will land when dropped. It uses
    /// a muted, semi-transparent style to distinguish it from real tabs.
    ///
    /// ### Arguments
    /// - `dragged`: The dragged tab data (used for title and modified state)
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `AnyElement`: The rendered ghost tab element
    fn render_ghost_tab(
        &self,
        slot: usize,
        dragged: &DraggedTab,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let modified_indicator = if dragged.is_modified { " •" } else { "" };
        div()
            .id(("ghost-tab", slot))
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_r_1()
            .border_b_0()
            .border_color(cx.theme().border)
            .bg(cx.theme().tab_active)
            .opacity(0.45)
            .child(
                div()
                    .pl_1()
                    .text_sm()
                    .text_color(cx.theme().tab_active_foreground)
                    .child(format!("{}{}", dragged.title, modified_indicator)),
            )
            .on_drop(cx.listener(|this, dragged: &DraggedTab, window, cx| {
                if let Some((slot, _)) = this.drag_ghost.take() {
                    this.handle_tab_drop(dragged, slot, window, cx);
                }
            }))
            .into_any_element()
    }

    /// Render all tabs, inserting a ghost tab at the current drag insertion point.
    ///
    /// During a drag operation, a ghost tab is rendered at the slot determined by
    /// the most recent `on_drag_move` event. The ghost is suppressed for no-op
    /// positions (where the tab would not actually move).
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Vec<AnyElement>`: Tab elements, with a ghost tab inserted at the drag target
    fn render_tabs_with_slots(&self, cx: &mut Context<Self>) -> Vec<AnyElement> {
        let ghost = if cx.has_active_drag() {
            self.drag_ghost.as_ref().and_then(|(slot, dragged)| {
                let from = dragged.tab_index;
                let is_noop = *slot == from || *slot == from + 1;
                if is_noop {
                    None
                } else {
                    Some((*slot, dragged))
                }
            })
        } else {
            None
        };
        let capacity = self.tabs.len() + if ghost.is_some() { 1 } else { 0 };
        let mut elements: Vec<AnyElement> = Vec::with_capacity(capacity);
        if let Some((0, dragged)) = ghost {
            elements.push(self.render_ghost_tab(0, dragged, cx));
        }
        for (index, tab) in self.tabs.iter().enumerate() {
            elements.push(self.render_tab(index, tab, cx));
            if let Some((slot, dragged)) = ghost
                && slot == index + 1
            {
                elements.push(self.render_ghost_tab(slot, dragged, cx));
            }
        }
        elements
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
        let has_file_path = file_path.is_some();
        let is_editor_tab = tab.as_editor().is_some();
        let cached_file_size = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_size_bytes)
            .map(components_utils::format_file_size);
        let cached_last_modified = tab
            .as_editor()
            .and_then(|editor_tab| editor_tab.file_last_modified)
            .and_then(components_utils::format_system_time);
        let mut tab_div = div()
            .id(("tab", tab_id))
            .flex()
            .items_center()
            .h(TAB_BAR_HEIGHT)
            .px_2()
            .gap_2()
            .border_r_1()
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
                let file_size = cached_file_size.clone();
                let last_modified = cached_last_modified.clone();
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
        let is_markdown_preview = tab.as_markdown_preview().is_some();
        let title: SharedString = tab.title().to_string().into();
        let is_modified = tab.is_modified();
        let mut tab_with_content = tab_div
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
            );
        if !is_markdown_preview {
            let is_source = cx.has_active_drag()
                && self
                    .drag_ghost
                    .as_ref()
                    .map(|(_, d)| d.tab_index == index)
                    .unwrap_or(false);
            if is_source {
                tab_with_content = tab_with_content.opacity(0.45);
            }
            tab_with_content = tab_with_content
                .on_drag(
                    DraggedTab {
                        tab_index: index,
                        title,
                        is_modified,
                    },
                    |dragged, _, _, cx| cx.new(|_| dragged.clone()),
                )
                .on_drag_move::<DraggedTab>(cx.listener(
                    move |this, event: &DragMoveEvent<DraggedTab>, _window, cx| {
                        let cursor = event.event.position;
                        let bounds = event.bounds;
                        if cursor.x < bounds.origin.x
                            || cursor.x > bounds.origin.x + bounds.size.width
                            || cursor.y < bounds.origin.y
                            || cursor.y > bounds.origin.y + bounds.size.height
                        {
                            return;
                        }
                        let slot = if cursor.x < bounds.origin.x + bounds.size.width * 0.5 {
                            index
                        } else {
                            index + 1
                        };
                        let dragged = event.drag(cx).clone();
                        this.drag_ghost = Some((slot, dragged));
                        cx.notify();
                    },
                ))
                .on_drop(cx.listener(|this, dragged: &DraggedTab, window, cx| {
                    if let Some((slot, _)) = this.drag_ghost.take() {
                        this.handle_tab_drop(dragged, slot, window, cx);
                    }
                }));
        }
        let tab_with_content = tab_with_content.context_menu(move |this, _window, _cx| {
            this.menu_with_disabled(
                crate::fulgur::ui::components_utils::reveal_in_file_manager_label(),
                Box::new(ShowInFileManager(index)),
                !has_file_path,
            )
            .menu_with_disabled(
                "Duplicate Tab",
                Box::new(DuplicateTab(index)),
                !is_editor_tab,
            )
            .separator()
            .menu("Close Tab", Box::new(CloseTabAction(tab_id)))
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
                total_tabs <= 1,
            )
        });
        tab_with_content.into_any_element()
    }
}

/// Reveals a file in the platform's native file manager with the file selected.
///
/// - **macOS**: `open -R <path>` — reveals and selects the file in Finder
/// - **Windows**: `explorer /select,<path>` — selects the file in Explorer
/// - **Linux**: falls back to opening the parent directory via the `open` crate,
///   as there is no universal "reveal" command across desktop environments
///
/// ### Arguments
/// - `file_path`: The path of the file to reveal
///
/// ### Returns
/// - `Ok(())` on success, `Err` with an error message on failure
fn reveal_file_in_file_manager(file_path: &std::path::Path) -> Result<(), String> {
    #[cfg(target_os = "macos")]
    {
        std::process::Command::new("open")
            .arg("-R")
            .arg(file_path)
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(target_os = "windows")]
    {
        std::process::Command::new("explorer")
            .arg(format!("/select,{}", file_path.display()))
            .spawn()
            .map(|_| ())
            .map_err(|e| e.to_string())
    }

    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let parent = file_path
            .parent()
            .ok_or_else(|| "File has no parent directory".to_string())?;
        open::that(parent).map_err(|e| e.to_string())
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::Fulgur;
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, tab::Tab, window_manager::WindowManager,
    };
    use gpui::{
        AppContext, Context, Entity, IntoElement, Render, TestAppContext, VisualTestContext,
        Window, div,
    };
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    struct EmptyView;

    impl Render for EmptyView {
        fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
            div()
        }
    }

    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur);
                    cx.new(|_| EmptyView)
                })
            })
            .expect("failed to open test window");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    // ========== get_tab_display_title tests ==========

    #[gpui::test]
    fn test_get_tab_display_title_returns_filename_for_unique_path(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                if let Some(Tab::Editor(e)) = this.tabs.first_mut() {
                    e.file_path = Some(PathBuf::from("/projects/foo/main.rs"));
                }
                let tab = this.tabs.first().unwrap();
                let (filename, folder) = this.get_tab_display_title(0, tab);
                assert_eq!(filename, "main.rs");
                assert!(
                    folder.is_none(),
                    "unique filename should have no parent folder suffix"
                );
            });
        });
    }

    #[gpui::test]
    fn test_get_tab_display_title_shows_parent_folder_for_duplicate_filenames(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                if let Some(Tab::Editor(e)) = this.tabs.first_mut() {
                    e.file_path = Some(PathBuf::from("/projects/a/main.rs"));
                }
                this.new_tab(window, cx);
                if let Some(Tab::Editor(e)) = this.tabs.get_mut(1) {
                    e.file_path = Some(PathBuf::from("/projects/b/main.rs"));
                }
                let tab0 = this.tabs.first().unwrap();
                let (filename0, folder0) = this.get_tab_display_title(0, tab0);
                assert_eq!(filename0, "main.rs");
                assert_eq!(
                    folder0.as_deref(),
                    Some("../a"),
                    "first tab should show its parent folder when filename is shared"
                );
                let tab1 = this.tabs.get(1).unwrap();
                let (filename1, folder1) = this.get_tab_display_title(1, tab1);
                assert_eq!(filename1, "main.rs");
                assert_eq!(
                    folder1.as_deref(),
                    Some("../b"),
                    "second tab should show its own parent folder"
                );
            });
        });
    }

    #[gpui::test]
    fn test_get_tab_display_title_returns_tab_title_for_untitled_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                // The default tab has no file_path; its display title should be the tab's own title
                let tab = this.tabs.first().unwrap();
                let tab_title = tab.title().to_string();
                let (display_title, folder) = this.get_tab_display_title(0, tab);
                assert_eq!(display_title, tab_title);
                assert!(folder.is_none());
            });
        });
    }

    // ========== on_next_tab tests ==========

    #[gpui::test]
    fn test_on_next_tab_advances_active_index_by_one(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                // Three tabs: move to index 0, then advance
                this.set_active_tab(0, window, cx);
                this.on_next_tab(window, cx);
                assert_eq!(this.active_tab_index, Some(1));
            });
        });
    }

    #[gpui::test]
    fn test_on_next_tab_wraps_around_from_last_to_first(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                let last = this.tabs.len() - 1;
                this.set_active_tab(last, window, cx);
                this.on_next_tab(window, cx);
                assert_eq!(this.active_tab_index, Some(0));
            });
        });
    }

    #[gpui::test]
    fn test_on_next_tab_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);

        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.on_next_tab(window, cx);
                assert_eq!(this.active_tab_index, None);
            });
        });
    }

    // ========== on_previous_tab tests ==========

    #[gpui::test]
    fn test_on_previous_tab_moves_to_previous_index(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                let last = this.tabs.len() - 1;
                this.set_active_tab(last, window, cx);
                this.on_previous_tab(window, cx);
                assert_eq!(this.active_tab_index, Some(last - 1));
            });
        });
    }

    #[gpui::test]
    fn test_on_previous_tab_wraps_around_from_first_to_last(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.new_tab(window, cx);
                this.new_tab(window, cx);
                this.set_active_tab(0, window, cx);
                this.on_previous_tab(window, cx);
                let last = this.tabs.len() - 1;
                assert_eq!(this.active_tab_index, Some(last));
            });
        });
    }

    #[gpui::test]
    fn test_on_previous_tab_is_noop_when_no_active_tab(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.active_tab_index = None;
                this.on_previous_tab(window, cx);
                assert_eq!(this.active_tab_index, None);
            });
        });
    }
}
