use gpui::{Action, App};
use gpui_component::{Theme, ThemeMode, ThemeRegistry};
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;

use crate::fulgur::settings::Settings;

// Embed bundled themes into the binary
#[derive(RustEmbed)]
#[folder = "./src/themes"]
#[include = "*.json"]
struct BundledThemes;

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
                eprintln!("Failed to extract bundled themes: {}", e);
            }
            path
        }
        Err(e) => {
            eprintln!("Failed to get themes directory: {}", e);
            return;
        }
    };
    if let Err(err) = ThemeRegistry::watch_dir(themes_directory, cx, move |cx| {
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme);
        }
        on_themes_loaded(cx);
    }) {
        eprintln!("Failed to watch themes directory: {}", err);
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
fn themes_directory_path() -> anyhow::Result<PathBuf> {
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
