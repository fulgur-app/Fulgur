use crate::fulgur::settings::{Settings, Themes};
use crate::fulgur::shared_state::SharedAppState;

use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};
use gpui_kit::{Action, App, BorrowAppContext, SharedString};
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;

// Embed bundled themes into the binary
#[derive(RustEmbed)]
#[folder = "./src/themes"]
#[include = "*.json"]
pub struct BundledThemes;

/// Apply a theme configuration and propagate it to every theming layer
///
/// ### Arguments
/// - `config`: The theme configuration to apply
/// - `cx`: The application context
pub fn apply_theme_config(config: &Rc<ThemeConfig>, cx: &mut App) {
    let theme = Theme::global_mut(cx);
    theme.apply_config(config);
    theme.focus_ring = false;
    Theme::change(config.mode, None, cx);
}

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
                log::error!("Failed to extract bundled themes: {e}");
            }
            path
        }
        Err(e) => {
            log::error!("Failed to get themes directory: {e}");
            return;
        }
    };
    if let Err(err) = ThemeRegistry::watch_dir(themes_directory, cx, move |cx| {
        apply_selected_theme(&theme_name, cx);
        on_themes_loaded(cx);
    }) {
        log::error!("Failed to watch themes directory: {err}");
    }
    if let Some(scrollbar_show) = scrollbar_show {
        Theme::set_scrollbar_mode(scrollbar_show, cx);
    }
    cx.refresh_windows();
    cx.on_action(|switch: &SwitchThemeMode, cx| {
        let mode = switch.0;
        Theme::change(mode, None, cx);
        cx.refresh_windows();
    });
}

/// Apply the selected theme if the theme registry knows it, otherwise keep the current theme
///
/// ### Arguments
/// - `theme_name`: The name of the theme selected in the settings
/// - `cx`: The application context
fn apply_selected_theme(theme_name: &SharedString, cx: &mut App) {
    if let Some(theme) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
        apply_theme_config(&theme, cx);
    }
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
        if !file_path.exists()
            && let Some(content) = BundledThemes::get(&file)
        {
            fs::write(&file_path, content.data.as_ref())?;
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
    crate::fulgur::utils::paths::config_subdir("themes")
}

/// Reload themes and publish them to `SharedAppState`: this function initializes the theme registry, reloads themes from disk, and publishes the theme catalog through `cx.update_global` so every window's observer picks it up.
///
/// ### Arguments
/// - `settings`: The application settings
/// - `cx`: The application context
pub fn reload_themes_and_update(settings: &Settings, cx: &mut App) {
    init(settings, cx, publish_themes);
}

/// Load the theme catalog from disk and publish it to `SharedAppState`, keeping the previous catalog when loading fails
///
/// ### Arguments
/// - `cx`: The application context
fn publish_themes(cx: &mut App) {
    match Themes::load() {
        Ok(themes) => cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.themes = Some(themes);
        }),
        Err(e) => log::error!("Failed to load themes: {e}"),
    }
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);

#[cfg(test)]
mod tests {
    use super::{BundledThemes, extract_bundled_themes};
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    /// Build a unique temporary directory path for theme-related tests.
    ///
    /// ### Arguments
    /// - `prefix`: A descriptive prefix included in the generated directory name.
    ///
    /// ### Returns
    /// - `PathBuf`: A path under `std::env::temp_dir()` that should be unique per call.
    fn temp_test_dir(prefix: &str) -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock should be after unix epoch")
            .as_nanos();
        std::env::temp_dir().join(format!("{}_{}_{}", prefix, std::process::id(), timestamp))
    }

    #[test]
    fn extract_bundled_themes_writes_missing_embedded_files() {
        let themes_dir = temp_test_dir("fulgur_extract_themes");
        extract_bundled_themes(&themes_dir).expect("extracting bundled themes should succeed");
        let bundled_files: Vec<String> = BundledThemes::iter().map(|f| f.to_string()).collect();
        assert!(
            !bundled_files.is_empty(),
            "bundled theme set should include files to extract"
        );
        for file in bundled_files {
            assert!(
                themes_dir.join(file).exists(),
                "missing extracted bundled theme file"
            );
        }
        fs::remove_dir_all(&themes_dir).expect("temporary themes test directory should be removed");
    }

    #[test]
    fn extract_bundled_themes_preserves_existing_file_contents() {
        let themes_dir = temp_test_dir("fulgur_extract_preserve");
        fs::create_dir_all(&themes_dir).expect("temporary themes test directory should be created");
        let existing_file_name = BundledThemes::iter()
            .next()
            .expect("bundled themes should include at least one file")
            .to_string();
        let existing_file_path = themes_dir.join(existing_file_name);
        let sentinel = r#"{"sentinel":"keep-existing-content"}"#;
        fs::write(&existing_file_path, sentinel)
            .expect("existing test theme file should be written");
        extract_bundled_themes(&themes_dir).expect("extracting bundled themes should succeed");
        let after_content = fs::read_to_string(&existing_file_path)
            .expect("existing theme file should remain readable");
        assert_eq!(
            after_content, sentinel,
            "existing file contents must not be overwritten by extraction"
        );
        fs::remove_dir_all(&themes_dir).expect("temporary themes test directory should be removed");
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_tests {
    use super::{
        BundledThemes, apply_selected_theme, extract_bundled_themes, publish_themes,
        themes_directory_path,
    };
    use crate::fulgur::shared_state::SharedAppState;
    use crate::test_support::setup_test_globals;
    use gpui_kit::component::{ActiveTheme, ThemeRegistry};
    use gpui_kit::{BorrowAppContext, SharedString, TestAppContext};

    const SELECTED_THEME: &str = "Catppuccin Latte";

    /// Register the bundled Catppuccin themes without starting the theme directory watcher,
    /// whose OS thread would wake GPUI tasks from outside the deterministic test scheduler.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context holding the theme registry
    fn register_bundled_catppuccin_themes(cx: &mut TestAppContext) {
        let theme_file =
            BundledThemes::get("catppuccin.json").expect("catppuccin theme should be bundled");
        let content = std::str::from_utf8(theme_file.data.as_ref())
            .expect("bundled theme file should be valid UTF-8");
        cx.update(|cx| {
            ThemeRegistry::global_mut(cx)
                .load_themes_from_str(content)
                .expect("bundled catppuccin theme should parse");
        });
    }

    #[gpui_kit::test]
    fn test_apply_selected_theme_applies_registered_theme(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        register_bundled_catppuccin_themes(cx);
        let selected_theme = SharedString::from(SELECTED_THEME);
        cx.update(|cx| {
            apply_selected_theme(&selected_theme, cx);
            assert_eq!(
                cx.theme().theme_name(),
                &selected_theme,
                "the selected registered theme should become the active theme"
            );
        });
    }

    #[gpui_kit::test]
    fn test_apply_selected_theme_keeps_current_theme_when_name_is_unknown(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        register_bundled_catppuccin_themes(cx);
        cx.update(|cx| {
            let theme_before = cx.theme().theme_name().clone();
            apply_selected_theme(&SharedString::from("Missing Theme"), cx);
            assert_eq!(
                cx.theme().theme_name(),
                &theme_before,
                "an unknown theme name should leave the active theme untouched"
            );
        });
    }

    #[gpui_kit::test]
    fn test_publish_themes_refreshes_shared_theme_state(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let themes_dir = themes_directory_path().expect("themes directory should resolve");
        extract_bundled_themes(&themes_dir).expect("bundled themes should be extracted");
        cx.update(|cx| {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.themes = None;
            });
            publish_themes(cx);
            assert!(
                cx.global::<SharedAppState>().themes.is_some(),
                "publishing themes should repopulate the shared theme catalog"
            );
        });
    }
}
