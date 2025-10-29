use anyhow::anyhow;
use gpui::*;
use gpui_component::{button::*, input::*, *};
use rust_embed::RustEmbed;
use std::borrow::Cow;
#[cfg(target_os = "windows")]
use gpui_component::menu::AppMenuBar;
mod themes;

// Asset loader for icons
#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

// Define actions for the app menus
actions!(lightspeed, [About, Quit, CloseWindow, NewFile, OpenFile, SaveFileAs, SaveFile, CloseFile]);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = lightspeed, no_json)]
pub struct SwitchTheme(pub SharedString);

fn build_menus(cx: &App) -> Vec<Menu> {
    let themes = ThemeRegistry::global(cx).sorted_themes();
    vec![
        Menu {
            name: "Lightspeed".into(),
            items: vec![
                MenuItem::Submenu(Menu {
                    name: "Theme".into(),
                    items: themes
                        .iter()
                        .map(|theme| MenuItem::action(theme.name.clone(), SwitchTheme(theme.name.clone())))
                        .collect(),
                }),
                MenuItem::action("About Lightspeed", About),
                MenuItem::Separator,
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", NewFile),
                MenuItem::action("Open...", OpenFile),
                MenuItem::separator(),
                MenuItem::action("Save as...", SaveFileAs),
                MenuItem::action("Save", SaveFile),
                MenuItem::separator(),
                MenuItem::action("Close file", CloseFile),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", gpui_component::input::Undo),
                MenuItem::action("Redo", gpui_component::input::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", gpui_component::input::Cut),
                MenuItem::action("Copy", gpui_component::input::Copy),
                MenuItem::action("Paste", gpui_component::input::Paste),
            ],
        },
        Menu {
            name: "Window".into(),
            items: vec![MenuItem::action("Close Window", CloseWindow)],
        },
    ]
}

// Custom title bar with platform-specific menu bar placement
pub struct CustomTitleBar {
    #[cfg(target_os = "windows")]
    app_menu_bar: Entity<AppMenuBar>,
}

impl CustomTitleBar {
    fn new(_window: &mut Window, _cx: &mut App) -> Entity<Self> {
        #[cfg(target_os = "windows")]
        let app_menu_bar = AppMenuBar::new(_window, _cx);

        _cx.new(|_cx| Self {
            #[cfg(target_os = "windows")]
            app_menu_bar,
        })
    }
}

impl Render for CustomTitleBar {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let mut title_bar = TitleBar::new();

        // Left side - menu bar on Windows only
        #[cfg(target_os = "windows")]
        {
            title_bar = title_bar.child(
                div()
                    .flex()
                    .items_center()
                    .child(self.app_menu_bar.clone()),
            );
        }
        #[cfg(not(target_os = "windows"))]
        {
            title_bar = title_bar.child(div());
        }

        title_bar
            // Center - app title (using absolute positioning to center)
            .child(
                div()
                    .absolute()
                    .left_0()
                    .right_0()
                    .flex()
                    .justify_center()
                    .items_center()
                    .child(
                        div()
                            .text_sm()
                            .font_semibold()
                            .text_color(cx.theme().foreground)
                            .child("Lightspeed"),
                    ),
            )
            // Right side - empty for now, window controls are automatically added by TitleBar
            .child(div())
    }
}

// Represents a single editor tab with its content
#[derive(Clone)]
pub struct EditorTab {
    id: usize,
    title: SharedString,
    content: Entity<InputState>,
    _file_path: Option<std::path::PathBuf>,
    _modified: bool,
}

impl EditorTab {
    fn new(id: usize, title: impl Into<SharedString>, window: &mut Window, cx: &mut App) -> Self {
        let content = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .placeholder("Start typing...")
        });
        
        Self {
            id,
            title: title.into(),
            content,
            _file_path: None,
            _modified: false,
        }
    }

    fn from_file(
        id: usize,
        path: std::path::PathBuf,
        contents: String,
        window: &mut Window,
        cx: &mut App,
    ) -> Self {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled")
            .to_string();

        let content = cx.new(|cx| {
            InputState::new(window, cx)
                .multi_line()
                .default_value(contents)
        });

        Self {
            id,
            title: file_name.into(),
            content,
            _file_path: Some(path),
            _modified: false,
        }
    }
}

pub struct Lightspeed {
    focus_handle: FocusHandle,
    title_bar: Entity<CustomTitleBar>,
    tabs: Vec<EditorTab>,
    active_tab_index: usize,
    next_tab_id: usize,
}

impl Lightspeed {
    fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
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
        if self.tabs.len() == 1 {
            // Don't close the last tab
            return;
        }

        if let Some(pos) = self.tabs.iter().position(|t| t.id == tab_id) {
            self.tabs.remove(pos);
            
            // Adjust active index if needed
            if self.active_tab_index >= self.tabs.len() {
                self.active_tab_index = self.tabs.len() - 1;
            } else if pos < self.active_tab_index {
                self.active_tab_index -= 1;
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

fn main() {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        
        // Initialize themes with callback to set menus after themes are loaded
        themes::init(cx, |cx| {
            cx.set_menus(build_menus(cx));
        });
        
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
        ]);
        
        // Handle theme switching from menu
        cx.on_action(|switch: &SwitchTheme, cx| {
            let theme_name = switch.0.clone();
            if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
                Theme::global_mut(cx).apply_config(&theme_config);
            }
            cx.refresh_windows();
        });

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                // Enable custom title bar
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                window.set_window_title("Lightspeed");
                let view = Lightspeed::new(window, cx);
                // Focus the view so keyboard shortcuts work immediately
                view.focus_handle(cx).focus(window);
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view.into(), window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}