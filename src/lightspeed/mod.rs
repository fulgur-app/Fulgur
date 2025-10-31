mod titlebar;
mod menus;
mod editor_tab;
mod themes;

use titlebar::CustomTitleBar;
use menus::*;
use editor_tab::EditorTab;

use gpui::*;
use std::ops::DerefMut;
use gpui_component::{ActiveTheme, ContextModal, IconName, Root, Sizable, StyledExt, Theme, ThemeRegistry, button::{Button, ButtonVariants}, h_flex, input::TextInput};

pub struct Lightspeed {
    focus_handle: FocusHandle,
    title_bar: Entity<CustomTitleBar>,
    tabs: Vec<EditorTab>,
    active_tab_index: usize,
    next_tab_id: usize,
}

impl Lightspeed {
    // Create a new Lightspeed instance
    // @param window: The window to create the Lightspeed instance in
    // @param cx: The application context
    // @return: The new Lightspeed instance
    pub fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let title_bar = CustomTitleBar::new(window, cx);

        // Create initial tab
        let initial_tab = EditorTab::new(0, "Untitled", window, cx);

        cx.new(|cx| {
            let entity = Self {
                focus_handle: cx.focus_handle(),
                title_bar,
                tabs: vec![initial_tab],
                active_tab_index: 0,
                next_tab_id: 1,
            };
            entity
        })
    }

    // Initialize the Lightspeed instance
    // @param cx: The application context
    pub fn init(cx: &mut App) {
        themes::init(cx, |cx| {

            // Set up keyboard shortcuts
            cx.bind_keys([
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-o", OpenFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-o", OpenFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-n", NewFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-n", NewFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-w", CloseFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-w", CloseFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-w", CloseAllFiles, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-w", CloseAllFiles, None),
                KeyBinding::new("cmd-q", Quit, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-q", Quit, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-s", SaveFile, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-s", SaveFile, None),
                #[cfg(target_os = "macos")]
                KeyBinding::new("cmd-shift-s", SaveFileAs, None),
                #[cfg(not(target_os = "macos"))]
                KeyBinding::new("ctrl-shift-s", SaveFileAs, None),
            ]);
            
            let menus = build_menus(cx);
            cx.set_menus(menus);
        });
    }

    // Create a new tab
    // @param window: The window to create the tab in
    // @param cx: The application context
    fn new_tab(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let tab = EditorTab::new(
            self.next_tab_id,
            format!("Untitled {}", self.next_tab_id),
            window,
            cx,
        );
        self.tabs.push(tab);
        self.active_tab_index = self.tabs.len() - 1;
        self.next_tab_id += 1;
        
        self.focus_active_tab(window, cx);
        cx.notify();
    }

    // Close a tab
    // @param tab_id: The ID of the tab to close
    // @param window: The window to close the tab in
    // @param cx: The application context
    fn close_tab(&mut self, tab_id: usize, window: &mut Window, cx: &mut Context<Self>) {

        if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
            if let Some(to_be_removed) = self.tabs.get_mut(pos) {
                // Check if the tab has been modified
                let is_modified = to_be_removed.check_modified(cx);
                if is_modified {
                    // Get the entity reference to use in the modal callbacks
                    let entity = cx.entity().clone();
                    
                    window.open_modal(cx.deref_mut(), move |modal, _, _| {
                        // Clone entity for on_ok closure
                        let entity_ok = entity.clone();
                        
                        // Return the modal builder
                        modal
                            .confirm()
                            .child("Are you sure you want to close this tab? Your changes will be lost.")
                            .on_ok(move |_, window, cx| {
                                // Remove the tab and adjust indices
                                entity_ok.update(cx, |this, cx| {
                                    if let Some(pos) = this.tabs.iter().position(|t| t.id == tab_id) {
                                        this.tabs.remove(pos);
                                        this.close_tab_manage_focus(window, cx, pos);
                                        cx.notify();
                                    }
                                });
                                
                                // Defer focus until after modal closes
                                entity_ok.update(cx, |_this, cx| {
                                    cx.defer_in(window, move |this, window, cx| {
                                        this.focus_active_tab(window, cx);
                                    });
                                });
                                
                                true
                            })
                            .on_cancel(move |_, _, _| {
                                // Just dismiss the modal without doing anything
                                true
                            })
                    });
                    return;
                }
            }
            self.tabs.remove(pos);
            self.close_tab_manage_focus(window, cx, pos);
            self.focus_active_tab(window, cx);
            cx.notify();
        }
    }

    // Close a tab and manage the focus
    // @param window: The window to close the tab in
    // @param cx: The application context
    // @param pos: The position of the tab to close
    fn close_tab_manage_focus(&mut self, window: &mut Window, cx: &mut Context<Self>, pos: usize) {
        // If no tabs left, create a new one
        if self.tabs.is_empty() {
            let new_tab = EditorTab::new(self.next_tab_id, "Untitled", window, cx);
            self.tabs.push(new_tab);
            self.next_tab_id += 1;
            self.active_tab_index = 0;
        } else {
            // Adjust active index
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            } else if pos < self.active_tab_index {
                self.active_tab_index -= 1;
            }
        }
        
        self.focus_active_tab(window, cx);
    }

    // Set the active tab
    // @param index: The index of the tab to set as active
    // @param window: The window to set the active tab in
    // @param cx: The application context
    fn set_active_tab(&mut self, index: usize, window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            self.focus_active_tab(window, cx);
            cx.notify();
        }
    }

    // Focus the active tab's content
    // @param window: The window to focus the tab in
    // @param cx: The application context
    pub fn focus_active_tab(&self, window: &mut Window, cx: &App) {
        if let Some(active_tab) = self.tabs.get(self.active_tab_index) {
            let focus_handle = active_tab.content.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
        }
    }

    // Close all tabs
    // @param window: The window to close all tabs in
    // @param cx: The application context
    fn close_all_tabs(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() > 0 {
            self.tabs.clear();
            self.active_tab_index = 0;
            self.next_tab_id = 1;
            cx.notify();
        }
    }

    // Open a file
    // @param window: The window to open the file in
    // @param cx: The application context
    fn open_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let path_future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: None,
        });

        cx.spawn_in(window, async move |view, window| {
            // Wait for the user to select a path
            let paths = path_future.await.ok()?.ok()??;
            let path = paths.first()?.clone();

            // Read file contents
            let contents = std::fs::read_to_string(&path).ok()?;

            // Update the view to add a new tab with the file
            window
                .update(|window, cx| {
                    _ = view.update(cx, |this, cx| {
                        let tab = EditorTab::from_file(
                            this.next_tab_id,
                            path.clone(),
                            contents,
                            window,
                            cx,
                        );
                        this.tabs.push(tab);
                        this.active_tab_index = this.tabs.len() - 1;
                        this.next_tab_id += 1;
                        this.focus_active_tab(window, cx);
                        cx.notify();
                    });
                })
                .ok();

            Some(())
        })
        .detach();
    }

    // Save a file
    // @param window: The window to save the file in
    // @param cx: The application context
    fn save_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }

        let active_tab = &self.tabs[self.active_tab_index];
        
        // If no path exists, use save_as instead
        if active_tab.file_path.is_none() {
            self.save_file_as(window, cx);
            return;
        }

        let path = active_tab.file_path.clone().unwrap();
        let content_entity = active_tab.content.clone();
        
        // Get the text content from the InputState
        let contents = content_entity.read(cx).text().to_string();
        
        // Write to file
        if let Err(e) = std::fs::write(&path, contents) {
            eprintln!("Failed to save file: {}", e);
            return;
        }

        // Mark as saved
        self.tabs[self.active_tab_index].mark_as_saved(cx);
        cx.notify();
    }

    // Save a file as
    // @param window: The window to save the file as in
    // @param cx: The application context
    fn save_file_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.is_empty() {
            return;
        }

        let active_tab_index = self.active_tab_index;
        let content_entity = self.tabs[active_tab_index].content.clone();
        
        // Get the current directory or use home directory
        let directory = if let Some(ref path) = self.tabs[active_tab_index].file_path {
            path.parent().unwrap_or(std::path::Path::new(".")).to_path_buf()
        } else {
            std::env::current_dir().unwrap_or_default()
        };

        let path_future = cx.prompt_for_new_path(&directory, None);

        cx.spawn_in(window, async move |view, window| {
            // Wait for the user to select a path
            let path = path_future.await.ok()?.ok()??;

            // Get the text content
            let contents = window
                .update(|_, cx| content_entity.read(cx).text().to_string())
                .ok()?;

            // Write to file
            if let Err(e) = std::fs::write(&path, &contents) {
                eprintln!("Failed to save file: {}", e);
                return None;
            }

            // Update the tab with the new path
            window
                .update(|_, cx| {
                    _ = view.update(cx, |this, cx| {
                        if let Some(tab) = this.tabs.get_mut(active_tab_index) {
                            tab.file_path = Some(path.clone());
                            tab.title = path
                                .file_name()
                                .and_then(|n| n.to_str())
                                .unwrap_or("Untitled")
                                .to_string()
                                .into();
                            tab.mark_as_saved(cx);
                            cx.notify();
                        }
                    });
                })
                .ok()?;

            Some(())
        })
        .detach();
    }

    // Update the modified status of the tabs
    // @param cx: The application context
    fn update_modified_status(&mut self, cx: &mut Context<Self>) {
        for tab in self.tabs.iter_mut() {
            tab.check_modified(cx);
        }
    }

    // Quit the application
    // @param window: The window to quit the application in
    // @param cx: The application context
    fn quit(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        // if self.tabs.len() > 0 {
        //     // Prompt the user to save the tabs if they are modified
        //     for tab in self.tabs.iter() {
        //         if tab.modified {
        //             println!("Tab {} is modified", tab.title); // TODO: Prompt the user to save the tab
        //         }
        //     }
        // }
        cx.quit();
    }
}

impl Focusable for Lightspeed {
    fn focus_handle(&self, _cx: &App) -> FocusHandle {
        self.focus_handle.clone()
    }
}

// Create a tab bar button
// @param id: The ID of the button
// @param tooltip: The tooltip of the button
// @param icon: The icon of the button
// @param border_color: The color of the border
// @return: The tab bar button
fn tab_bar_button_factory(id: &'static str, tooltip: &'static str, icon: IconName, border_color: Hsla) -> Button {
    Button::new(id)
        .icon(icon)
        .ghost()
        .xsmall()
        .tooltip(tooltip)
        .border_t_0()   
        .border_l_0()
        .border_r_1()
        .border_b_1()
        .border_color(border_color)
        .corner_radii(Corners {
            top_left: px(0.0),
            top_right: px(0.0),
            bottom_left: px(0.0),
            bottom_right: px(0.0),
        })
        .h(px(40.))
        .w(px(40.))
        .cursor_pointer()
}

// Create a status bar item
// @param content: The content of the status bar item
// @param border_color: The color of the border
// @return: The status bar item
fn status_bar_item_factory(content: String, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

// Create a status bar right item
// @param content: The content of the status bar right item
// @param border_color: The color of the border
// @return: The status bar right item
fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color).border_l_1()
}

// Create a status bar left item
// @param content: The content of the status bar left item
// @param border_color: The color of the border
// @return: The status bar left item
fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color).border_r_1()
}

impl Render for Lightspeed {
    // Render the Lightspeed instance
    // @param window: The window to render the Lightspeed instance in
    // @param cx: The application context
    // @return: The rendered Lightspeed instance
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        // Ensure we always have at least one tab
        if self.tabs.is_empty() {
            let new_tab = EditorTab::new(self.next_tab_id, "Untitled", window, cx);
            self.tabs.push(new_tab);
            self.next_tab_id += 1;
            self.active_tab_index = 0;
        }
        
        // Update modified status of tabs
        self.update_modified_status(cx);
        let cursor_pos = self.tabs[self.active_tab_index].content.read(cx).cursor_position();
        let active_tab = self.tabs.get(self.active_tab_index);

        // Render modal, drawer, and notification layers
        let modal_layer = Root::render_modal_layer(window, cx);
        let drawer_layer = Root::render_drawer_layer(window, cx);
        let notification_layer = Root::render_notification_layer(window, cx);

        div()
            .size_full()
            .child(
                div()
            .size_full()
            .v_flex()
            .track_focus(&self.focus_handle)
            .on_action(cx.listener(|this, _action: &NewFile, window, cx| {
                this.new_tab(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &OpenFile, window, cx| {
                this.open_file(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &CloseFile, window, cx| {
                this.close_tab(this.active_tab_index, window, cx);
            }))
            .on_action(cx.listener(|this, _action: &CloseAllFiles, window, cx| {
                this.close_all_tabs(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &SaveFile, window, cx| {
                this.save_file(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &SaveFileAs, window, cx| {
                this.save_file_as(window, cx);
            }))
            .on_action(cx.listener(|this, _action: &Quit, window, cx| {
                this.quit(window, cx);
            }))
            .on_action(cx.listener(|_this, _action: &SwitchTheme, _window, cx| {
                let theme_name = _action.0.clone();
                if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
                    Theme::global_mut(cx).apply_config(&theme_config);
                    }
                    cx.refresh_windows();
                }))
            .child(self.title_bar.clone())
            .child(
                // Tab bar with + button and tabs
                div()
                    .flex()
                    .items_center()
                    .h(px(40.))
                    .bg(cx.theme().tab_bar)
                    //.border_b_1()
                    //.border_color(cx.theme().border)
                    .child(
                        // + button to create new tabs
                        tab_bar_button_factory("new-tab", "New Tab", IconName::Plus, cx.theme().border)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.new_tab(window, cx);
                            })),
                    )
                    .child(
                        // + button to create new tabs
                        tab_bar_button_factory("open-file", "Open File", IconName::FolderOpen, cx.theme().border)
                            .on_click(cx.listener(|this, _, window, cx| {
                                this.open_file(window, cx);
                            })),
                    )
                    .child(
                        // TabBar with all tabs
                        div()
                            .flex()
                            .flex_1()
                            .items_center()
                            .children(self.tabs.iter().enumerate().map(|(index, tab)| {
                                let tab_id = tab.id;
                                let is_active = index == self.active_tab_index;

                                let mut tab_div = div()
                                    .flex()
                                    .items_center()
                                    .h(px(40.))
                                    .px_2()
                                    .gap_2()
                                    .border_r_1()
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .on_mouse_down(MouseButton::Left, cx.listener(move |this, _, window, cx| {
                                        if !is_active {
                                            this.set_active_tab(index, window, cx);
                                        }
                                    }));

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
                                            .child(format!("{}{}", 
                                                tab.title.clone(),
                                                if tab.modified { " •" } else { "" }
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
                            }))

                            .child(
                                div()
                                    .flex_1()
                                    .min_w(px(0.))
                                    .border_b_1()
                                    .border_color(cx.theme().border)
                                    .h(px(40.))
                            )
                    )
                )
            .child(
                // Active tab content area
                {
                    let mut content_div = div()
                        .flex_1()
                        .p_0()
                        .m_0()
                        .overflow_hidden();
                    
                    if let Some(tab) = active_tab {
                        content_div = content_div.child(
                            TextInput::new(&tab.content)
                                .w_full()
                                .h_full()
                                .border_0()
                                .text_size(px(14.))
                        );
                    }
                    
                    content_div
                }
            )
            .child(
                h_flex()
                    .justify_between()
                    .bg(cx.theme().background)
                    .px_2()
                    .border_t_1()
                    .border_color(cx.theme().border)
                    .text_color(cx.theme().muted_foreground)
                    .child(div()
                        .flex()
                        .justify_start()
                        .child(
                            status_bar_left_item_factory(format!("Ln {}, Col {}", 132, 22), cx.theme().border)
                        )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .child(status_bar_right_item_factory(format!("Ln {}, Col {}", 123, 48), cx.theme().border))
                            .child(status_bar_right_item_factory(format!("Ln {}, Col {}", cursor_pos.line + 1, cursor_pos.character + 1), cx.theme().border)),
                    )
                )
            )
            .children(drawer_layer)
            .children(modal_layer)
            .children(notification_layer)
    }
}