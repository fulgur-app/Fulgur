use std::fs;

use gpui::*;
use gpui_component::{
    ActiveTheme, Sizable, Size, StyledExt,
    button::{Button, ButtonVariants},
    group_box::GroupBoxVariant,
    h_flex,
    setting::{
        NumberFieldOptions, SettingField, SettingGroup, SettingItem, SettingPage,
        Settings as SettingsComponent,
    },
    v_flex,
};

use crate::fulgur::{
    Fulgur, crypto_helper,
    settings::{AppSettings, EditorSettings, Themes},
    sync::sync::perform_initial_synchronization,
    themes,
    ui::{icons::CustomIcon, menus::build_menus},
};

const DEVICE_KEY_PLACEHOLDER: &str = "<Device Key>";

#[derive(Clone)]
pub struct SettingsTab {
    pub id: usize,
    pub title: SharedString,
}

impl SettingsTab {
    /// Create a new settings tab
    ///
    /// ### Arguments
    /// - `id`: The ID of the settings tab
    /// - `_window`: The window
    /// - `_cx`: The context
    ///
    /// ### Returns
    /// - `Self`: The settings tab
    pub fn new(id: usize, _window: &mut Window, _cx: &mut App) -> Self {
        Self {
            id,
            title: SharedString::from("Settings"),
        }
    }
}

impl Fulgur {
    /// Create the Editor settings page
    ///
    /// ### Arguments
    /// - `entity`: The Fulgur entity
    ///
    /// ### Returns
    /// - `SettingPage`: The Editor settings page
    fn create_editor_page(entity: Entity<Self>) -> SettingPage {
        let default_editor_settings = EditorSettings::new();
        SettingPage::new("Editor").default_open(true).groups(vec![
            SettingGroup::new().title("Font").items(vec![
                SettingItem::new(
                    "Font Size",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 8.0,
                            max: 24.0,
                            step: 2.0,
                            ..Default::default()
                        },
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity.read(cx).settings.editor_settings.font_size as f64
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: f64, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.editor_settings.font_size = val as f32;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.font_size as f64),
                )
                .description("Adjust the font size for the editor (8-24)."),
            ]),
            SettingGroup::new().title("Indentation").items(vec![
                SettingItem::new(
                    "Tab Size",
                    SettingField::number_input(
                        NumberFieldOptions {
                            min: 2.0,
                            max: 12.0,
                            step: 2.0,
                            ..Default::default()
                        },
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.editor_settings.tab_size as f64
                        },
                        {
                            let entity = entity.clone();
                            move |val: f64, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.editor_settings.tab_size = val as usize;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.tab_size as f64),
                )
                .description("Number of spaces for indentation. Requires restart."),
                SettingItem::new(
                    "Show Indent Guides",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity.read(cx).settings.editor_settings.show_indent_guides
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.editor_settings.show_indent_guides = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.show_indent_guides),
                )
                .description("Show vertical lines indicating indentation levels."),
            ]),
            SettingGroup::new().title("Display").items(vec![
                SettingItem::new(
                    "Show Line Numbers",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity.read(cx).settings.editor_settings.show_line_numbers
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.editor_settings.show_line_numbers = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.show_line_numbers),
                )
                .description("Display line numbers in the editor gutter."),
                SettingItem::new(
                    "Soft Wrap",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.editor_settings.soft_wrap
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.editor_settings.soft_wrap = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.soft_wrap),
                )
                .description("Wrap long lines to the next line instead of scrolling."),
            ]),
            SettingGroup::new().title("Markdown").items(vec![
                SettingItem::new(
                    "Default Show Preview",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .editor_settings
                                    .markdown_settings
                                    .show_markdown_preview
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings
                                        .editor_settings
                                        .markdown_settings
                                        .show_markdown_preview = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(
                        default_editor_settings
                            .markdown_settings
                            .show_markdown_preview,
                    ),
                )
                .description("Show preview when opening Markdown files."),
                SettingItem::new(
                    "Show Toolbar by default",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .editor_settings
                                    .markdown_settings
                                    .show_markdown_toolbar
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings
                                        .editor_settings
                                        .markdown_settings
                                        .show_markdown_toolbar = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(
                        default_editor_settings
                            .markdown_settings
                            .show_markdown_toolbar,
                    ),
                )
                .description("Show toolbar by default when opening Markdown files."),
            ]),
            SettingGroup::new().title("File Monitoring").items(vec![
                SettingItem::new(
                    "Watch Files",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.editor_settings.watch_files
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.editor_settings.watch_files = val;
                                    if val {
                                        this.start_file_watcher();
                                    } else {
                                        this.stop_file_watcher();
                                    }
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_editor_settings.watch_files),
                )
                .description("Monitor files for external changes."),
            ]),
        ])
    }

    /// Create the Application settings page
    ///
    /// ### Arguments
    /// - `entity`: The Fulgur entity
    ///
    /// ### Returns
    /// - `SettingPage`: The Application settings page
    fn create_application_page(entity: Entity<Self>) -> SettingPage {
        let default_app_settings = AppSettings::new();

        SettingPage::new("Application")
            .default_open(true)
            .groups(vec![
                SettingGroup::new().title("General").items(vec![
                    SettingItem::new(
                        "Confirm Exit",
                        SettingField::switch(
                            {
                                let entity = entity.clone();
                                move |cx: &App| entity.read(cx).settings.app_settings.confirm_exit
                            },
                            {
                                let entity = entity.clone();
                                move |val: bool, cx: &mut App| {
                                    entity.update(cx, |this, cx| {
                                        this.settings.app_settings.confirm_exit = val;
                                        if let Err(e) = this.update_and_propagate_settings(cx) {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                }
                            },
                        )
                        .default_value(default_app_settings.confirm_exit),
                    )
                    .description("Show confirmation dialog before exiting the application."),
                ]),
                SettingGroup::new().title("Synchronization").items(vec![
                    SettingItem::new(
                        "Activate Synchronization",
                        SettingField::switch(
                            {
                                let entity = entity.clone();
                                move |cx: &App| {
                                    entity
                                        .read(cx)
                                        .settings
                                        .app_settings
                                        .synchronization_settings
                                        .is_synchronization_activated
                                }
                            },
                            {
                                let entity = entity.clone();
                                move |val: bool, cx: &mut App| {
                                    entity.update(cx, |this, cx| {
                                            this.settings
                                                .app_settings
                                                .synchronization_settings
                                                .is_synchronization_activated = val;
                                            if val {
                                                if let Err(e) =
                                                    crypto_helper::check_private_public_keys(&mut this.settings)
                                                {
                                                    log::error!("Failed to check private/public keys: {}", e);
                                                }
                                            }
                                            if let Err(e) = this.update_and_propagate_settings(cx) {
                                                log::error!("Failed to save settings: {}", e);
                                            }
                                        });
                                    if val {
                                        perform_initial_synchronization(entity.clone(), cx);
                                    }
                                }
                            },
                        )
                        .default_value(default_app_settings.confirm_exit),
                    )
                    .description("Activate synchronization with the server."),
                    SettingItem::new(
                        "Server URL",
                        SettingField::input(
                            {
                                let entity = entity.clone();
                                move |cx: &App| {
                                    entity
                                        .read(cx)
                                        .settings
                                        .app_settings
                                        .synchronization_settings
                                        .server_url
                                        .as_ref()
                                        .map(|s| SharedString::from(s.clone()))
                                        .unwrap_or_default()
                                }
                            },
                            {
                                let entity = entity.clone();
                                move |val: SharedString, cx: &mut App| {
                                    entity.update(cx, |this, cx| {
                                        let url = if val.is_empty() {
                                            None
                                        } else {
                                            Some(val.to_string())
                                        };
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .server_url = url;
                                        if let Err(e) = this.update_and_propagate_settings(cx) {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                        this.restart_sse_connection(cx);
                                    });
                                }
                            },
                        )
                        .default_value(
                            default_app_settings
                                .synchronization_settings
                                .server_url
                                .clone()
                                .map(|s| SharedString::from(s))
                                .unwrap_or_default(),
                        ),
                    )
                    .description("URL of the synchronization server."),
                    SettingItem::new(
                        "Email",
                        SettingField::input(
                            {
                                let entity = entity.clone();
                                move |cx: &App| {
                                    entity
                                        .read(cx)
                                        .settings
                                        .app_settings
                                        .synchronization_settings
                                        .email
                                        .as_ref()
                                        .map(|s| SharedString::from(s.clone()))
                                        .unwrap_or_default()
                                }
                            },
                            {
                                let entity = entity.clone();
                                move |val: SharedString, cx: &mut App| {
                                    entity.update(cx, |this, cx| {
                                        let email = if val.is_empty() {
                                            None
                                        } else {
                                            Some(val.to_string())
                                        };
                                        this.settings.app_settings.synchronization_settings.email =
                                            email;
                                        if let Err(e) = this.update_and_propagate_settings(cx) {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                        this.restart_sse_connection(cx);
                                    });
                                }
                            },
                        )
                        .default_value(
                            default_app_settings
                                .synchronization_settings
                                .email
                                .clone()
                                .map(|s| SharedString::from(s))
                                .unwrap_or_default(),
                        ),
                    )
                    .description("Email for synchronization."),
                    SettingItem::new(
                        "Device Key",
                        SettingField::input(
                            move |_cx: &App| SharedString::from(DEVICE_KEY_PLACEHOLDER),
                            {
                                let entity = entity.clone();
                                move |val: SharedString, cx: &mut App| {
                                    entity.update(cx, |this, cx| {
                                        let key = if val.is_empty() {
                                            None
                                        } else if val.to_string() == DEVICE_KEY_PLACEHOLDER {
                                            return;
                                        } else {
                                            Some(val.to_string())
                                        };
                                        if let Err(e) =
                                            crypto_helper::save_device_api_key_to_keychain(key)
                                        {
                                            log::error!("Failed to save device API key: {}", e);
                                        } else {
                                            log::info!("Device API key saved successfully");
                                            {
                                                let mut token_state =
                                                    this.shared_state(cx).token_state.lock();
                                                token_state.access_token = None;
                                                token_state.token_expires_at = None;
                                                log::debug!(
                                                    "Cleared cached access token for new device"
                                                );
                                            }
                                        }
                                        this.restart_sse_connection(cx);
                                    });
                                }
                            },
                        ),
                    )
                    .description(
                        "Device Key for synchronization (stored in keychain).",
                    ),
                    SettingItem::render({
                        let entity = entity.clone();
                        move |_options, _window, _cx| {
                            h_flex()
                                .w_full()
                                .justify_end()
                                .mt_2()
                                .child(
                                    Button::new("begin-synchronization-button")
                                        .label("Begin Synchronization")
                                        .primary()
                                        .small()
                                        .cursor_pointer()
                                        .on_click({
                                            let entity = entity.clone();
                                            move |_, _window, cx| {
                                                let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
                                                {
                                                    let mut token_state = shared.token_state.lock();
                                                    token_state.access_token = None;
                                                    token_state.token_expires_at = None;
                                                    log::debug!("Cleared cached token before manual synchronization");
                                                }
                                                perform_initial_synchronization(entity.clone(), cx);
                                            }
                                        }),
                                )
                                .into_any_element()
                        }
                    }),
                ]),
            ])
    }

    /// Create the Themes settings page
    ///
    /// ### Arguments
    /// - `entity`: The Fulgur entity
    /// - `themes`: The themes to display
    ///
    /// ### Returns
    /// - `SettingPage`: The Themes settings page
    fn create_themes_page(entity: Entity<Self>, themes: &Themes) -> SettingPage {
        let mut user_theme_items = Vec::new();
        let mut default_theme_items = Vec::new();
        for theme in &themes.user_themes {
            let theme_name = theme.name.clone();
            let theme_author = theme.author.clone();
            let theme_path = theme.path.clone();
            let themes_info = theme
                .themes
                .iter()
                .map(|t| format!("{} ({})", t.name, t.mode))
                .collect::<Vec<String>>()
                .join(", ");
            let button_id = format!("delete-theme-{}", theme_name);
            let button_id_static: &'static str = Box::leak(button_id.into_boxed_str());
            user_theme_items.push(SettingItem::render({
                let entity = entity.clone();
                move |_options, _window, cx| {
                    let theme_path = theme_path.clone();
                    let entity_clone = entity.clone();
                    h_flex()
                        .w_full()
                        .justify_between()
                        .gap_3()
                        .child(
                            v_flex()
                                .gap_1()
                                .child(
                                    div()
                                        .font_semibold()
                                        .child(format!("{} by {}", theme_name, theme_author)),
                                )
                                .child(
                                    div()
                                        .text_xs()
                                        .text_color(cx.theme().muted_foreground)
                                        .child(themes_info.clone()),
                                ),
                        )
                        .child(
                            Button::new(button_id_static)
                                .icon(CustomIcon::Close)
                                .small()
                                .cursor_pointer()
                                .on_click(move |_, _window, cx| {
                                    if let Err(e) = fs::remove_file(&theme_path) {
                                        log::error!(
                                            "Failed to delete theme file {}: {}",
                                            theme_path.display(),
                                            e
                                        );
                                    } else {
                                        log::info!("Deleted theme file: {:?}", theme_path);
                                    }
                                    let entity_for_update = entity_clone.clone();
                                    entity_clone.update(cx, |this, cx| {
                                        themes::reload_themes_and_update(
                                            &this.settings,
                                            entity_for_update,
                                            cx,
                                        );
                                    });
                                }),
                        )
                        .into_any_element()
                }
            }));
        }
        for theme in &themes.default_themes {
            let theme_name = theme.name.clone();
            let theme_author = theme.author.clone();
            let themes_info = theme
                .themes
                .iter()
                .map(|t| format!("{} ({})", t.name, t.mode))
                .collect::<Vec<String>>()
                .join(", ");
            default_theme_items.push(SettingItem::render(move |_options, _window, cx| {
                v_flex()
                    .w_full()
                    .gap_1()
                    .child(
                        div()
                            .font_semibold()
                            .child(format!("{} by {} (Default)", theme_name, theme_author)),
                    )
                    .child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child(themes_info.clone()),
                    )
                    .into_any_element()
            }));
        }
        let mut groups = Vec::new();
        if !user_theme_items.is_empty() {
            groups.push(
                SettingGroup::new()
                    .title("User Themes")
                    .items(user_theme_items),
            );
        }
        if !default_theme_items.is_empty() {
            groups.push(
                SettingGroup::new()
                    .title("Default Themes")
                    .items(default_theme_items),
            );
        }
        SettingPage::new("Themes").groups(groups)
    }

    /// Create settings pages using the Settings component
    ///
    /// ### Arguments
    /// - `window`: The window
    /// - `cx`: The context
    ///
    /// ### Returns
    /// - `Vec<SettingPage>`: The settings pages
    fn create_settings_pages(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Vec<SettingPage> {
        let entity = cx.entity();
        let mut pages = vec![
            Self::create_editor_page(entity.clone()),
            Self::create_application_page(entity.clone()),
        ];
        let themes = self.shared_state(cx).themes.lock().clone();
        if let Some(ref themes) = themes {
            pages.push(Self::create_themes_page(entity, themes));
        }
        pages
    }

    /// Render the settings
    ///
    /// ### Arguments
    /// - `window`: The window
    /// - `cx`: The context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The settings UI
    pub fn render_settings(&self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .id("settings-scroll-container")
            .size_full()
            .overflow_x_scroll()
            .child(
                v_flex()
                    .w(px(980.0))
                    .h_full()
                    .mx_auto()
                    .text_color(cx.theme().foreground)
                    .text_size(px(12.0))
                    .border_r_1()
                    .border_color(cx.theme().border)
                    .child(
                        SettingsComponent::new("fulgur-settings")
                            .with_size(Size::Medium)
                            .with_group_variant(GroupBoxVariant::Outline)
                            .pages(self.create_settings_pages(window, cx)),
                    ),
            )
    }

    /// Clear the recent files
    ///
    /// ### Arguments
    /// - `cx`: The context
    pub fn clear_recent_files(&mut self, cx: &mut Context<Self>) {
        self.settings.recent_files.clear();
        if let Err(e) = self.update_and_propagate_settings(cx) {
            log::error!("Failed to save settings: {}", e);
        }
        let menus = build_menus(
            &self.settings.recent_files.get_files(),
            self.shared_state(cx).update_link.lock().clone(),
        );
        cx.set_menus(menus);
    }
}
