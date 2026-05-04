use crate::fulgur::Fulgur;
use crate::fulgur::settings::{Settings, Themes};

use gpui::{Action, App, Entity};
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
        if let Some(theme) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme);
        }
        on_themes_loaded(cx);
    }) {
        log::error!("Failed to watch themes directory: {err}");
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

/// Reload themes and update the Fulgur instance: this function initializes the theme registry, reloads themes from disk, updates the Fulgur entity's themes field, and refreshes the window.
///
/// ### Arguments
/// - `settings`: The application settings
/// - `entity`: The Fulgur entity to update
/// - `cx`: The application context
pub fn reload_themes_and_update(settings: &Settings, entity: &Entity<Fulgur>, cx: &mut App) {
    let entity_clone = entity.clone();
    init(settings, cx, move |cx| {
        let themes = match Themes::load() {
            Ok(themes) => Some(themes),
            Err(e) => {
                log::error!("Failed to load themes: {e}");
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
    use super::init;
    use super::reload_themes_and_update;
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, SharedString, TestAppContext, WindowId, WindowOptions};
    use gpui_component::ActiveTheme;
    use gpui_component::ThemeRegistry;
    use parking_lot::Mutex;
    use std::{
        cell::RefCell,
        path::PathBuf,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
        time::Duration,
    };

    /// Wait until a condition becomes true while pumping the GPUI async runtime.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context to drive.
    /// - `attempts`: Maximum polling attempts before timing out.
    /// - `delay`: Sleep duration between attempts.
    /// - `predicate`: The condition to evaluate after each pump.
    ///
    /// ### Returns
    /// - `true`: The predicate returned true within the allowed attempts.
    /// - `false`: The predicate never became true before timeout.
    fn wait_until(
        cx: &mut TestAppContext,
        attempts: usize,
        delay: Duration,
        mut predicate: impl FnMut(&mut TestAppContext) -> bool,
    ) -> bool {
        for _ in 0..attempts {
            cx.run_until_parked();
            if predicate(cx) {
                return true;
            }
            thread::sleep(delay);
        }
        false
    }

    /// Initialize GPUI globals needed to construct `Fulgur` windows in tests.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context to initialize.
    fn setup_test_globals(cx: &mut TestAppContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
    }

    /// Open a test window that hosts a `Fulgur` root.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test context used to open the window.
    ///
    /// ### Returns
    /// - `(WindowId, Entity<Fulgur>)`: The opened window ID and associated `Fulgur` entity.
    fn open_window_with_fulgur(cx: &mut TestAppContext) -> (WindowId, Entity<Fulgur>) {
        let window_id_slot: RefCell<Option<WindowId>> = RefCell::new(None);
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        cx.update(|cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                let window_id = window.window_handle().window_id();
                let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                *window_id_slot.borrow_mut() = Some(window_id);
                *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
            })
            .expect("failed to open test window");
        });
        (
            window_id_slot
                .into_inner()
                .expect("failed to capture test window id"),
            fulgur_slot
                .into_inner()
                .expect("failed to capture test Fulgur entity"),
        )
    }

    #[gpui::test]
    fn test_init_runs_watch_load_callback_and_applies_selected_theme(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let selected_theme: SharedString = "Catppuccin Latte".into();
        let callback_count = Arc::new(AtomicUsize::new(0));
        let callback_count_for_init = callback_count.clone();
        let mut settings = Settings::new();
        settings.app_settings.theme = selected_theme.clone();
        cx.update(|cx| {
            init(&settings, cx, move |_| {
                callback_count_for_init.fetch_add(1, Ordering::SeqCst);
            });
        });
        assert!(
            wait_until(cx, 40, Duration::from_millis(25), |_| {
                callback_count.load(Ordering::SeqCst) > 0
            }),
            "theme watch initialization callback should run at least once"
        );
        cx.update(|cx| {
            assert!(
                ThemeRegistry::global(cx)
                    .themes()
                    .contains_key(&selected_theme),
                "selected bundled theme should be present after watched load"
            );
            assert_eq!(
                cx.theme().theme_name(),
                &selected_theme,
                "init callback should apply configured selected theme"
            );
        });
    }

    #[gpui::test]
    fn test_reload_themes_and_update_refreshes_shared_theme_state(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let (_, fulgur) = open_window_with_fulgur(cx);
        let settings = cx.update(|cx| fulgur.read(cx).settings.clone());
        cx.update(|cx| {
            let shared = cx.global::<SharedAppState>();
            *shared.themes.lock() = None;
        });
        cx.update(|cx| {
            reload_themes_and_update(&settings, &fulgur, cx);
        });
        assert!(
            wait_until(cx, 80, Duration::from_millis(25), |cx| {
                cx.update(|cx| {
                    let shared = cx.global::<SharedAppState>();
                    shared.themes.lock().is_some()
                })
            }),
            "reloading themes should repopulate shared theme catalog"
        );
    }
}
