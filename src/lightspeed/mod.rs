mod titlebar;
mod menus;
mod editor_tab;
mod themes;

pub use titlebar::CustomTitleBar;
pub use menus::*;
pub use editor_tab::EditorTab;

use gpui::*;
use gpui_component::{ActiveTheme, IconName, Sizable, StyledExt, button::{Button, ButtonVariants}, h_flex, input::TextInput};

pub struct Lightspeed {
    focus_handle: FocusHandle,
    title_bar: Entity<CustomTitleBar>,
    tabs: Vec<EditorTab>,
    active_tab_index: usize,
    next_tab_id: usize,
}

impl Lightspeed {
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

    pub fn init(cx: &mut App) {
        themes::init(cx, |cx| {
            let menus = build_menus(cx);
            cx.set_menus(menus);
        });
    }

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
        cx.notify();
    }

    fn close_tab(&mut self, tab_id: usize, _window: &mut Window, cx: &mut Context<Self>) {

        if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.remove(pos);
            
            // Adjust active index if needed
            if self.active_tab_index > 0 {
                if self.active_tab_index >= self.tabs.len() {
                    self.active_tab_index = self.tabs.len() - 1;
                } else if pos < self.active_tab_index {
                    self.active_tab_index -= 1;
                }
            }
            
            cx.notify();
        }
    }

    fn set_active_tab(&mut self, index: usize, _window: &mut Window, cx: &mut Context<Self>) {
        if index < self.tabs.len() {
            self.active_tab_index = index;
            cx.notify();
        }
    }

    fn close_all_tabs(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if self.tabs.len() > 0 {
            self.tabs.clear();
            self.active_tab_index = 0;
            self.next_tab_id = 1;
            cx.notify();
        }
    }

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
                        cx.notify();
                    });
                })
                .ok();

            Some(())
        })
        .detach();
    }

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

fn status_bar_item_factory(content: String, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color).border_l_1()
}

fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color).border_r_1()
}

impl Render for Lightspeed {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let active_tab = self.tabs.get(self.active_tab_index);

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
            .on_action(cx.listener(|this, _action: &Quit, window, cx| {
                this.quit(window, cx);
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
                                            .child(tab.title.clone()),
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
                            status_bar_left_item_factory(format!("Ln {}, Col {}", 123, 48), cx.theme().border)
                        )
                    )
                    .child(
                        div()
                            .flex()
                            .justify_end()
                            .child(status_bar_right_item_factory(format!("Ln {}, Col {}", 123, 48), cx.theme().border))
                            .child(status_bar_right_item_factory(format!("Ln {}, Col {}", 13, 22), cx.theme().border)),
                    )
                )
            
        
    }
}