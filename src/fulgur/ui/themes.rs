use crate::fulgur::Fulgur;
use crate::fulgur::settings::{Settings, Themes};
use gpui::*;
use gpui_component::{Theme, ThemeMode, ThemeRegistry};
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;

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
            entity_clone.update(cx, |fulgur, cx| {
                let shared = fulgur.shared_state(cx);
                *shared.themes.lock() = Some(themes);
            });
        }
    });
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);
