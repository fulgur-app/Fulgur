use gpui::{Action, App, Context, PathPromptOptions, SharedString, Window};
use gpui_component::notification::NotificationType;
use gpui_component::{Theme, ThemeMode, ThemeRegistry, WindowExt};
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;

use crate::fulgur::Fulgur;
use crate::fulgur::menus::build_menus;
use crate::fulgur::settings::{Settings, Themes};

// Embed bundled themes into the binary
#[derive(RustEmbed)]
#[folder = "./src/themes"]
#[include = "*.json"]
pub struct BundledThemes;

// Initialize the themes
// @param settings: The application settings containing theme preferences
// @param cx: The application context
// @param on_themes_loaded: The callback to call when the themes are loaded
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

// Extract bundled themes to the themes directory if they don't exist
// @param themes_dir: The themes directory path
// @return: Result indicating success or failure
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

// Get the path to the themes directory
// @return: The path to the themes directory
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

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);

impl Fulgur {
    // Add a theme to the themes directory. Prompt the user for the path to the theme file.
    // If the theme file is added successfully, reload the themes and update the menus.
    // @param window: The window context
    // @param cx: The application context
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
                            let recent_files = settings.recent_files.get_files().clone();
                            let entity_clone = entity.clone();
                            init(&settings, cx, move |cx| {
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
                                let menus = build_menus(cx, &recent_files);
                                cx.set_menus(menus);
                                cx.refresh_windows();
                            });
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
}
