use crate::fulgur::settings::{Settings, Themes};
use crate::fulgur::shared_state::SharedAppState;

use gpui_kit::base::input::CursorBlink;
use gpui_kit::component::{Theme, ThemeConfig, ThemeMode, ThemeRegistry};
use gpui_kit::{Action, App, BorrowAppContext, SharedString};
use rust_embed::RustEmbed;
use std::fs;
use std::path::PathBuf;
use std::rc::Rc;
use std::time::Duration;

/// Duration of each visible or hidden phase of a blinking caret
const CURSOR_BLINK_INTERVAL: Duration = Duration::from_millis(500);

/// Inactivity after which the caret stops blinking and stays visible, so an idle editor stops
/// re-rendering its window on every blink
pub(crate) const CURSOR_BLINK_IDLE_TIMEOUT: Duration = Duration::from_secs(10);

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

/// Build the caret blink policy matching the cursor blink setting
///
/// ### Arguments
/// - `cursor_blink`: Whether the caret should blink
///
/// ### Returns
/// - `CursorBlink`: A caret blinking every `CURSOR_BLINK_INTERVAL` that turns steady after
///   `CURSOR_BLINK_IDLE_TIMEOUT` without activity, or a steady caret when blinking is off
fn cursor_blink_policy(cursor_blink: bool) -> CursorBlink {
    if cursor_blink {
        CursorBlink::blinking(CURSOR_BLINK_INTERVAL).with_idle_timeout(CURSOR_BLINK_IDLE_TIMEOUT)
    } else {
        CursorBlink::steady()
    }
}

/// Apply the cursor blink setting to every caret, leaving the theme untouched when it already matches
///
/// ### Arguments
/// - `cursor_blink`: Whether the caret should blink
/// - `cx`: The application context
fn apply_cursor_blink(cursor_blink: bool, cx: &mut App) {
    let policy = cursor_blink_policy(cursor_blink);
    if Theme::global(cx).cursor_blink != policy {
        Theme::update(cx, |theme| theme.cursor_blink = policy);
    }
}

/// Reapply the cursor blink setting whenever the shared settings change
///
/// ### Arguments
/// - `cx`: The application context
fn observe_cursor_blink_setting(cx: &mut App) {
    cx.observe_global::<SharedAppState>(|cx| {
        let cursor_blink = cx
            .global::<SharedAppState>()
            .settings
            .editor_settings
            .cursor_blink;
        apply_cursor_blink(cursor_blink, cx);
    })
    .detach();
}

/// Initialize theme extraction, directory watching, and action listeners once at startup
///
/// ### Arguments
/// - `settings`: The application settings containing theme preferences
/// - `cx`: The application context
/// - `on_themes_loaded`: The callback to call when the themes are loaded
pub fn init(settings: &Settings, cx: &mut App, on_themes_loaded: impl Fn(&mut App) + 'static) {
    apply_cursor_blink(settings.editor_settings.cursor_blink, cx);
    observe_cursor_blink_setting(cx);
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
    cx.observe_global::<ThemeRegistry>(|cx| {
        if cx.has_global::<SharedAppState>() {
            reload_theme_catalog(cx);
        }
    })
    .detach();
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

/// Reload the catalog and reapply the current selection without repeating startup setup
///
/// ### Arguments
/// - `cx`: The application context containing the registry and shared settings
pub fn reload_theme_catalog(cx: &mut App) {
    match Themes::load() {
        Ok(themes) => {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.themes = Some(themes);
            });
            let theme_name = cx
                .global::<SharedAppState>()
                .settings
                .app_settings
                .theme
                .clone();
            apply_selected_theme(&theme_name, cx);
        }
        Err(e) => log::error!("Failed to load themes: {e}"),
    }
}

#[derive(Action, Clone, PartialEq)]
#[action(namespace = themes, no_json)]
pub(crate) struct SwitchThemeMode(pub(crate) ThemeMode);

#[cfg(test)]
mod tests {
    use super::{
        BundledThemes, CURSOR_BLINK_IDLE_TIMEOUT, CURSOR_BLINK_INTERVAL, cursor_blink_policy,
        extract_bundled_themes,
    };
    use std::{
        fs,
        path::PathBuf,
        time::{SystemTime, UNIX_EPOCH},
    };

    #[test]
    fn cursor_blink_policy_blinks_until_the_editor_goes_idle() {
        let policy = cursor_blink_policy(true);
        assert_eq!(policy.interval(), Some(CURSOR_BLINK_INTERVAL));
        assert_eq!(policy.idle_timeout(), Some(CURSOR_BLINK_IDLE_TIMEOUT));
    }

    #[test]
    fn cursor_blink_policy_is_steady_when_blinking_is_off() {
        let policy = cursor_blink_policy(false);
        assert!(policy.is_steady());
        assert_eq!(policy.idle_timeout(), None);
    }

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
        BundledThemes, CURSOR_BLINK_IDLE_TIMEOUT, apply_cursor_blink, apply_selected_theme,
        extract_bundled_themes, observe_cursor_blink_setting, reload_theme_catalog,
        themes_directory_path,
    };
    use crate::fulgur::shared_state::SharedAppState;
    use crate::test_support::setup_test_globals;
    use gpui_kit::base::input::CursorBlink;
    use gpui_kit::component::{ActiveTheme, Theme, ThemeRegistry};
    use gpui_kit::{App, BorrowAppContext, SharedString, TestAppContext};
    use std::cell::Cell;
    use std::rc::Rc;

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

    /// Verify repeated catalog reloads apply the selection currently stored in settings.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context
    #[gpui_kit::test]
    fn test_reload_theme_catalog_refreshes_catalog_and_current_selection(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        register_bundled_catppuccin_themes(cx);
        let themes_dir = themes_directory_path().expect("themes directory should resolve");
        extract_bundled_themes(&themes_dir).expect("bundled themes should be extracted");
        cx.update(|cx| {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.themes = None;
                shared.settings.app_settings.theme = SELECTED_THEME.into();
            });
            reload_theme_catalog(cx);
            assert!(
                cx.global::<SharedAppState>().themes.is_some(),
                "reloading themes should repopulate the shared theme catalog"
            );
            assert_eq!(cx.theme().theme_name().as_ref(), SELECTED_THEME);

            // A later refresh must use the current settings rather than the startup selection.
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.settings.app_settings.theme = "Catppuccin Mocha".into();
            });
            reload_theme_catalog(cx);
            assert_eq!(cx.theme().theme_name().as_ref(), "Catppuccin Mocha");
        });
    }

    /// Store the cursor blink setting in the shared settings, as the settings page does.
    ///
    /// ### Arguments
    /// - `cursor_blink`: The new value of the setting
    /// - `cx`: The application context holding `SharedAppState`
    fn set_cursor_blink_setting(cursor_blink: bool, cx: &mut App) {
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.settings.editor_settings.cursor_blink = cursor_blink;
        });
    }

    /// Read the caret blink policy from the base theme, which every input reads it from.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `CursorBlink`: The policy carets currently follow
    fn caret_blink_policy(cx: &App) -> CursorBlink {
        gpui_kit::base::Theme::global(cx).cursor_blink
    }

    /// Install the cursor blink policy and its settings observer as `init` does.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context
    fn start_cursor_blink_observer(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            apply_cursor_blink(true, cx);
            observe_cursor_blink_setting(cx);
        });
    }

    /// Verify toggling the setting switches every caret between blinking and steady.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context
    #[gpui_kit::test]
    fn test_cursor_blink_follows_the_shared_editor_setting(cx: &mut TestAppContext) {
        start_cursor_blink_observer(cx);
        cx.update(|cx| {
            assert_eq!(
                caret_blink_policy(cx).idle_timeout(),
                Some(CURSOR_BLINK_IDLE_TIMEOUT),
                "a blinking caret should stop blinking once the editor is idle"
            );
        });

        cx.update(|cx| set_cursor_blink_setting(false, cx));
        cx.update(|cx| {
            assert!(
                caret_blink_policy(cx).is_steady(),
                "turning the setting off should make the caret steady"
            );
        });

        cx.update(|cx| set_cursor_blink_setting(true, cx));
        cx.update(|cx| {
            assert_eq!(
                caret_blink_policy(cx).idle_timeout(),
                Some(CURSOR_BLINK_IDLE_TIMEOUT),
                "turning the setting back on should restore the blinking caret"
            );
        });
    }

    /// Verify a settings change that keeps the blink policy does not update the theme,
    /// which would refresh every window.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context
    #[gpui_kit::test]
    fn test_unrelated_settings_changes_leave_the_theme_untouched(cx: &mut TestAppContext) {
        start_cursor_blink_observer(cx);
        let theme_updates = Rc::new(Cell::new(0_usize));
        cx.update(|cx| {
            let theme_updates = theme_updates.clone();
            cx.observe_global::<Theme>(move |_| theme_updates.set(theme_updates.get() + 1))
                .detach();
        });

        cx.update(|cx| {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.settings.editor_settings.soft_wrap =
                    !shared.settings.editor_settings.soft_wrap;
            });
        });
        assert_eq!(
            theme_updates.get(),
            0,
            "an unrelated settings change should not touch the theme"
        );

        cx.update(|cx| set_cursor_blink_setting(false, cx));
        assert!(
            theme_updates.get() > 0,
            "changing the blink setting should update the theme"
        );
    }
}
