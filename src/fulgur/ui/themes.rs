use gpui::prelude::FluentBuilder;
use gpui::*;
use gpui_component::button::{Button, ButtonVariants};
use gpui_component::notification::NotificationType;
use gpui_component::scroll::ScrollableElement;
use gpui_component::{
    Disableable, Sizable, Theme, ThemeMode, ThemeRegistry, WindowExt, h_flex, v_flex,
};
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use crate::fulgur::Fulgur;
use crate::fulgur::settings::{Settings, Themes};

// Embed bundled themes into the binary
#[derive(RustEmbed)]
#[folder = "./src/themes"]
#[include = "*.json"]
pub struct BundledThemes;

/// Initialize the themes
///
/// ### Arguments
/// - `settings`: The application settings containing theme preferences
/// - `cx`: The application context
/// - `on_themes_loaded`: The callback to call when the themes are loaded
pub fn init(settings: &Settings, cx: &mut App, on_themes_loaded: impl Fn(&mut App) + 'static) {
    let theme_name = settings.app_settings.theme.clone();
    let scrollbar_show = settings.app_settings.scrollbar_show;
    let themes_directory = match themes_directory_path() {
        Ok(path) => {
            if let Err(e) = extract_bundled_themes(&path) {
                log::error!("Failed to extract bundled themes: {}", e);
            }
            path
        }
        Err(e) => {
            log::error!("Failed to get themes directory: {}", e);
            return;
        }
    };
    if let Err(err) = ThemeRegistry::watch_dir(themes_directory, cx, move |cx| {
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme);
        }
        on_themes_loaded(cx);
    }) {
        log::error!("Failed to watch themes directory: {}", err);
    }
    if let Some(scrollbar_show) = scrollbar_show {
        Theme::global_mut(cx).scrollbar_show = scrollbar_show;
    }
    cx.refresh_windows();
    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        Theme::change(mode, None, cx);
        cx.refresh_windows();
    });
}

/// Extract bundled themes to the themes directory if they don't exist
///
/// ### Arguments
/// - `themes_dir`: The themes directory path
///
/// ### Returns
/// - `Ok(())`: Result indicating success
/// - `Err(anyhow::Error)`: Result indicating failure
fn extract_bundled_themes(themes_dir: &PathBuf) -> anyhow::Result<()> {
    fs::create_dir_all(themes_dir)?;
    for file in BundledThemes::iter() {
        let file_path = themes_dir.join(file.as_ref());
        if !file_path.exists() {
            if let Some(content) = BundledThemes::get(&file) {
                fs::write(&file_path, content.data.as_ref())?;
            }
        }
    }
    Ok(())
}

/// Get the path to the themes directory
///
/// ### Returns
/// - `Ok(PathBuf)`: The path to the themes directory
/// - `Err(anyhow::Error)`: If the themes directory path could not be determined
pub fn themes_directory_path() -> anyhow::Result<PathBuf> {
    #[cfg(target_os = "windows")]
    {
        let app_data = std::env::var("APPDATA")?;
        let mut path = PathBuf::from(app_data);
        path.push("Fulgur");
        fs::create_dir_all(&path)?;
        path.push("themes");
        Ok(path)
    }

    #[cfg(not(target_os = "windows"))]
    {
        let home = std::env::var("HOME")?;
        let mut path = PathBuf::from(home);
        path.push(".fulgur");
        fs::create_dir_all(&path)?;
        path.push("themes");
        Ok(path)
    }
}

/// Reload themes and update the Fulgur instance: this function initializes the theme registry, reloads themes from disk, updates the Fulgur entity's themes field, and refreshes the window.
///
/// ### Arguments
/// - `settings`: The application settings
/// - `entity`: The Fulgur entity to update
/// - `cx`: The application context
pub fn reload_themes_and_update(settings: &Settings, entity: Entity<Fulgur>, cx: &mut App) {
    let entity_clone = entity.clone();
    init(settings, cx, move |cx| {
        let themes = match Themes::load() {
            Ok(themes) => Some(themes),
            Err(e) => {
                log::error!("Failed to load themes: {}", e);
                None
            }
        };
        if let Some(themes) = themes {
            entity_clone.update(cx, |fulgur, _cx| {
                fulgur.themes = Some(themes);
            });
        }
    });
}

/// Make a select theme item
///
/// ### Arguments
/// - `entity`: The entity
/// - `theme_name`: The name of the theme
/// - `is_current_theme`: Whether the theme is the current theme
/// - `current_theme_shared`: The shared state of the current theme
///
/// ### Returns
/// - `Div`: The select theme item
fn make_select_theme_item(
    entity: Entity<Fulgur>,
    theme_name: String,
    is_current_theme: bool,
    current_theme_shared: Arc<Mutex<String>>,
) -> Div {
    let button_id = format!("Select_{}", theme_name);
    let button_id = SharedString::from(button_id);
    h_flex()
        .justify_between()
        .py_1()
        .child(div().py_1().text_sm().child(theme_name.clone()))
        .when(!is_current_theme, |this| {
            this.child(
                Button::new(button_id.clone())
                    .child("Select")
                    .small()
                    .cursor_pointer()
                    .on_click(move |_this, _window, cx| {
                        if let Some(theme_config) = ThemeRegistry::global(cx)
                            .themes()
                            .get(theme_name.as_str())
                            .cloned()
                        {
                            Theme::global_mut(cx).apply_config(&theme_config);
                            let theme_name_clone = theme_name.clone();
                            entity.update(cx, |fulgur, cx| {
                                fulgur.settings.app_settings.theme =
                                    theme_name_clone.clone().into();
                                if let Err(e) = fulgur.settings.save() {
                                    log::error!("Failed to save settings: {}", e);
                                }
                                if let Ok(mut current) = current_theme_shared.lock() {
                                    *current = theme_name_clone;
                                }
                                cx.notify();
                            });
                            cx.refresh_windows();
                        }
                    }),
            )
        })
        .when(is_current_theme, |this| {
            this.child(
                Button::new(button_id)
                    .child("Select")
                    .primary()
                    .small()
                    .disabled(true),
            )
        })
}

/// Make a select theme list
///
/// ### Arguments
/// - `entity`: The entity
/// - `themes`: The list of themes
/// - `current_theme`: The name of the current theme
/// - `current_theme_shared`: The shared state of the current theme
///
/// ### Returns
/// - `Div`: The select theme list
fn make_select_theme_list(
    entity: Entity<Fulgur>,
    themes: Vec<String>,
    current_theme: String,
    current_theme_shared: Arc<Mutex<String>>,
) -> Div {
    let entity = entity.clone();
    div().rounded_md().children(
        themes
            .clone()
            .iter()
            .map(move |theme| {
                make_select_theme_item(
                    entity.clone(),
                    theme.clone(),
                    theme.to_string() == current_theme,
                    current_theme_shared.clone(),
                )
            })
            .collect::<Vec<Div>>(),
    )
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);

impl Fulgur {
    /// Add a theme to the themes directory. Prompt the user for the path to the theme file.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn add_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        let path_future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select theme".into()),
        });
        let settings = self.settings.clone();
        let entity = cx.entity();
        cx.spawn_in(window, async move |_view, window| {
            let paths = path_future.await.ok()?.ok()??;
            let theme_path = paths.first()?.clone();
            if theme_path.extension().and_then(|s| s.to_str()) != Some("json") {
                window
                    .update(|window, cx| {
                        window.push_notification(
                            "Invalid file type. Please select a JSON theme file.".to_string(),
                            cx,
                        );
                    })
                    .ok()?;
                return None;
            }
            let themes_dir = match themes_directory_path() {
                Ok(path) => path,
                Err(e) => {
                    log::error!("Failed to get themes directory: {}", e);
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from(format!(
                                "Failed to access themes directory: {}",
                                e
                            ));
                            window.push_notification((NotificationType::Error, notification), cx);
                        })
                        .ok()?;
                    return None;
                }
            };
            let filename = match theme_path.file_name() {
                Some(name) => name,
                None => {
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from("Invalid theme file path.");
                            window.push_notification((NotificationType::Error, notification), cx);
                        })
                        .ok()?;
                    return None;
                }
            };
            let dest_path = themes_dir.join(filename);
            match fs::copy(&theme_path, &dest_path) {
                Ok(_) => {
                    log::info!("Theme file copied to: {:?}", dest_path);
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from(format!(
                                "Theme '{}' added successfully!",
                                filename.to_string_lossy()
                            ));
                            window.push_notification((NotificationType::Success, notification), cx);
                            reload_themes_and_update(&settings, entity, cx);
                        })
                        .ok()?;
                }
                Err(e) => {
                    log::error!("Failed to copy theme file: {}", e);
                    window
                        .update(|window, cx| {
                            let notification =
                                SharedString::from(format!("Failed to add theme: {}", e));
                            window.push_notification((NotificationType::Error, notification), cx);
                        })
                        .ok()?;
                }
            }
            Some(())
        })
        .detach();
    }

    /// Open theme selector as a sheet (sliding panel from right side)
    ///     
    /// This is an alternative to the dialog-based theme selector
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn select_theme_sheet(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let current_theme = self.settings.app_settings.theme.to_string();
        let current_theme_shared = Arc::new(Mutex::new(current_theme.clone()));
        let viewport_height = window.viewport_size().height;
        window.open_sheet(cx, move |sheet, _window, cx| {
            let themes = ThemeRegistry::global(cx).sorted_themes();
            let light_themes: Vec<String> = themes
                .iter()
                .filter(|theme| theme.mode == ThemeMode::Light)
                .map(|theme| theme.name.to_string())
                .collect();
            let dark_themes: Vec<String> = themes
                .iter()
                .filter(|theme| theme.mode == ThemeMode::Dark)
                .map(|theme| theme.name.to_string())
                .collect();

            let entity_dark = entity.clone();
            let entity_light = entity.clone();
            let current_theme_shared_dark = current_theme_shared.clone();
            let current_theme_shared_light = current_theme_shared.clone();
            let current_theme_display = current_theme_shared
                .lock()
                .ok()
                .map(|t| t.clone())
                .unwrap_or(current_theme.clone());
            let max_height = px((viewport_height - px(150.0)).into()); //TODO: Make this dynamic based on the content
            sheet
                .title("Select Theme")
                .size(px(400.))
                .overlay(false)
                .child(
                    v_flex()
                        .overflow_y_scrollbar()
                        .gap_2()
                        .h(max_height)
                        .child(div().text_lg().child("Dark themes"))
                        .child(make_select_theme_list(
                            entity_dark,
                            dark_themes.clone(),
                            current_theme_display.clone(),
                            current_theme_shared_dark,
                        ))
                        .child(div().text_lg().mt_4().child("Light themes"))
                        .child(make_select_theme_list(
                            entity_light,
                            light_themes.clone(),
                            current_theme_display.clone(),
                            current_theme_shared_light,
                        )),
                )
                .footer({
                    let entity_footer = entity.clone();
                    h_flex()
                        .justify_between()
                        .w_full()
                        .child(
                            Button::new("add-theme-footer")
                                .child("Add new theme...")
                                .small()
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    entity_footer.update(cx, |this, cx| {
                                        this.add_theme(window, cx);
                                    });
                                    // Close sheet after opening add theme dialog
                                    // User can reopen to see new theme
                                    //window.close_sheet(cx);
                                }),
                        )
                        .child(
                            Button::new("ok-footer")
                                .child("OK")
                                .small()
                                .primary()
                                .cursor_pointer()
                                .on_click(|_, window, cx| {
                                    window.close_sheet(cx);
                                }),
                        )
                })
        });
    }
}
