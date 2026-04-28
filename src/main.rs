#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use fulgur::fulgur;
use gpui::{AppContext, AssetSource, BorrowAppContext, SharedString};
use parking_lot::Mutex;
use rust_embed::RustEmbed;
use std::{borrow::Cow, path::PathBuf, sync::Arc};
#[cfg(not(target_env = "msvc"))]
use tikv_jemallocator::Jemalloc;

#[cfg(not(target_env = "msvc"))]
#[global_allocator]
static GLOBAL: Jemalloc = Jemalloc;

#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
#[include = "icon_square.png"]
#[include = "icon.png"]
#[include = "icon.icns"]
#[include = "icon.ico"]
pub struct Assets;

impl AssetSource for Assets {
    /// Load an asset from the assets folder
    ///
    /// ### Arguments
    /// - `path`: The path to the asset
    ///
    /// ### Returns
    /// - `Result<Option<Cow<'static, [u8]>>>`: The asset data if found, otherwise None
    fn load(&self, path: &str) -> anyhow::Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(data) = Self::get(path) {
            return Ok(Some(data.data));
        }
        let path_without_prefix = path.strip_prefix("assets/").unwrap_or(path);
        if path_without_prefix != path
            && let Some(data) = Self::get(path_without_prefix)
        {
            return Ok(Some(data.data));
        }
        Ok(None)
    }

    /// List all assets in the assets folder
    ///
    /// ### Arguments
    /// - `path`: The path to the assets
    ///
    /// ### Returns
    /// - `Result<Vec<SharedString>>`: The list of assets
    fn list(&self, path: &str) -> anyhow::Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

/// Convert a file:// URL string to a PathBuf
///
/// ### Arguments
/// - `url_string`: The URL string to convert (e.g., "file:///Users/user/file.txt")
///
/// ### Returns
/// - `Some(PathBuf)`: The PathBuf if successful
/// - `None`: If the URL could not be converted to a path
fn url_to_path(url_string: &str) -> Option<PathBuf> {
    let path_str = url_string.strip_prefix("file://").unwrap_or(url_string);
    match urlencoding::decode(path_str) {
        Ok(decoded) => {
            let path = PathBuf::from(decoded.into_owned());
            if path.exists() && path.is_file() {
                Some(path)
            } else {
                log::warn!("URL converted to path, but target is not a valid file");
                None
            }
        }
        Err(e) => {
            log::error!("Failed to decode URL: {}", e);
            None
        }
    }
}

fn main() {
    // On Windows, set the Application User Model ID before any window is created
    // so that the taskbar button and the jump list share the same AUMID.
    #[cfg(target_os = "windows")]
    fulgur::utils::jump_list::set_app_user_model_id();

    if let Err(e) = fulgur::utils::logger::init() {
        eprintln!("Failed to initialize logger: {}", e);
    }
    if let Ok(config_dir) = fulgur::utils::paths::config_dir() {
        fulgur::utils::atomic_write::cleanup_orphan_temp_files(&config_dir);
    }
    let settings_load_result = fulgur::settings::Settings::load();
    let is_first_run = settings_load_result.is_err();
    let mut settings = settings_load_result.unwrap_or_else(|e| {
        eprintln!("Failed to load settings, using defaults: {}", e);
        fulgur::settings::Settings::new()
    });
    let debug_mode = settings.app_settings.debug_mode;
    fulgur::utils::logger::set_debug_mode(debug_mode);
    let current_version = env!("CARGO_PKG_VERSION");
    log::info!("=== Fulgur v{} Starting ===", current_version);
    log::info!("Platform: {}", std::env::consts::OS);
    log::info!("Architecture: {}", std::env::consts::ARCH);
    let args: Vec<String> = std::env::args().collect();
    log::info!("Command-line arguments: {:?}", args);
    if args.len() > 1 {
        log::debug!("File to open from command-line: {}", args[1]);
    }
    // Check for jump-list command flags before collecting file paths.
    #[cfg(target_os = "windows")]
    {
        let ipc_cmd = if args.iter().any(|a| a == "--new-tab") {
            Some("new-tab")
        } else if args.iter().any(|a| a == "--new-window") {
            Some("new-window")
        } else {
            None
        };
        if let Some(cmd) = ipc_cmd
            && fulgur::utils::single_instance::try_send_command_to_existing_instance(cmd)
        {
            return;
        }
    }

    let cli_file_paths: Vec<PathBuf> = args
        .iter()
        .skip(1)
        .filter_map(|arg| {
            // Skip our own jump-list flags so they aren't treated as file paths.
            if arg == "--new-tab" || arg == "--new-window" {
                return None;
            }
            let path = PathBuf::from(arg);
            if path.exists() && path.is_file() {
                Some(path)
            } else {
                if !arg.is_empty() {
                    log::warn!("Invalid or non-existent file argument");
                }
                None
            }
        })
        .collect();

    // On Windows, if we have file paths AND another Fulgur is already running,
    // forward the paths to it and exit — that instance will open/focus the files.
    #[cfg(target_os = "windows")]
    if !cli_file_paths.is_empty()
        && fulgur::utils::single_instance::try_forward_to_existing_instance(&cli_file_paths)
    {
        return;
    }

    let app = gpui_platform::application().with_assets(Assets);
    let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_files_clone = pending_files.clone();
    app.on_open_urls(move |urls: Vec<String>| {
        log::debug!("Received {} file URL(s) from macOS open event", urls.len());
        let file_paths: Vec<PathBuf> = urls.iter().filter_map(|url| url_to_path(url)).collect();

        if file_paths.is_empty() {
            log::warn!("No valid file paths from macOS open event");
            return;
        }
        log::debug!(
            "Processing {} valid file(s) from macOS open event",
            file_paths.len()
        );
        {
            let mut pending = pending_files_clone.lock();
            pending.extend(file_paths);
            log::debug!(
                "Added files to pending queue, total pending: {}",
                pending.len()
            );
        }
    });
    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        if is_first_run {
            let appearance = cx.window_appearance();
            let is_dark = matches!(
                appearance,
                gpui::WindowAppearance::Dark | gpui::WindowAppearance::VibrantDark
            );
            settings.app_settings.theme = if is_dark {
                "Default Dark".into()
            } else {
                "Default Light".into()
            };
            log::info!(
                "First run: OS appearance is {:?}, applying theme \"{}\"",
                appearance,
                settings.app_settings.theme
            );
        }
        fulgur::Fulgur::init(cx, &mut settings);
        let shared_state =
            fulgur::shared_state::SharedAppState::new(settings, pending_files.clone());
        cx.set_global(shared_state);
        // On Windows, start the IPC listener now that SharedAppState is registered.
        // We grab the two arcs from the global so there's a single source of truth.
        #[cfg(target_os = "windows")]
        {
            let shared = cx.global::<fulgur::shared_state::SharedAppState>();
            let pf = shared.pending_files_from_macos.clone();
            let pic = shared.pending_ipc_commands.clone();
            fulgur::utils::single_instance::start_ipc_listener(pf, pic);
        }
        cx.set_global(fulgur::window_manager::WindowManager::new());
        let windows_state = fulgur::state_persistence::WindowsState::load().ok();
        if let Some(ws) = windows_state.filter(|ws| !ws.windows.is_empty()) {
            log::info!("Restoring {} saved window(s)", ws.windows.len());
            for (index, window_state) in ws.windows.into_iter().enumerate() {
                let cli_files = if index == 0 {
                    cli_file_paths.clone()
                } else {
                    vec![]
                };
                let saved_bounds = Some(window_state.window_bounds);
                cx.spawn(async move |cx| create_window(cx, index, saved_bounds, cli_files).await)
                    .detach();
            }
        } else {
            log::info!("No saved state, creating initial window");
            cx.spawn(async move |cx| create_window(cx, 0, None, cli_file_paths).await)
                .detach();
        }
    });
}

/// Create a new window
///
/// ### Arguments
/// * `cx` - The application context
/// * `window_index` - The index of the window to create
/// * `saved_bounds` - Previously loaded window bounds for this window, if any
/// * `cli_file_paths` - The paths of the files to open in the window
async fn create_window(
    cx: &mut gpui::AsyncApp,
    window_index: usize,
    saved_bounds: Option<fulgur::state_persistence::SerializedWindowBounds>,
    cli_file_paths: Vec<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (window_bounds, saved_display_id) = match saved_bounds {
        Some(ref b) => (Some(b.to_gpui_bounds()), b.display_id),
        None => (None, None),
    };
    let display_id = if let Some(saved_id) = saved_display_id {
        cx.update(|cx| {
            cx.displays()
                .into_iter()
                .find(|display| {
                    let display_id_u32: u32 = display.id().into();
                    display_id_u32 == saved_id
                })
                .map(|display| display.id())
        })
    } else {
        None
    };
    let window_options = gpui::WindowOptions {
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        window_bounds,
        display_id,
        #[cfg(target_os = "linux")]
        app_id: Some("Fulgur".to_string()),
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    };
    let window = cx.open_window(window_options, |window, cx| {
        window.set_window_title("Fulgur");
        let window_id = window.window_handle().window_id();
        let view = fulgur::Fulgur::new(window, cx, window_id, window_index);
        cx.update_global::<fulgur::window_manager::WindowManager, _>(|manager, _| {
            manager.register(window_id, view.downgrade());
        });
        let view_clone = view.clone();
        window.on_window_should_close(cx, move |window, cx| {
            view_clone.update(cx, |fulgur, cx| {
                fulgur.on_window_close_requested(window, cx)
            })
        });
        if !cli_file_paths.is_empty() {
            log::debug!(
                "Processing {} command-line file arguments",
                cli_file_paths.len()
            );
            for file_path in cli_file_paths.iter() {
                view.update(cx, |fulgur, cx| {
                    fulgur.handle_open_file_from_cli(window, cx, file_path.clone());
                });
            }
        } else {
            view.update(cx, |fulgur, cx| fulgur.focus_active_tab(window, cx));
        }
        cx.new(|cx| gpui_component::Root::new(view, window, cx))
    })?;
    window.update(cx, |_, window, _| {
        window.activate_window();
    })?;

    // Check for updates on first window only
    if window_index == 0 {
        let update_info = cx.update(|cx| {
            cx.global::<fulgur::shared_state::SharedAppState>()
                .update_info
                .clone()
        });
        let current_version = env!("CARGO_PKG_VERSION").to_string();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(5));
            log::info!("Checking for updates...");
            match fulgur::utils::updater::check_for_updates(current_version) {
                Ok(Some(new_update_info)) => {
                    *update_info.lock() = Some(new_update_info);
                }
                Ok(None) => {}
                Err(e) => {
                    log::warn!("Failed to check for updates: {}", e);
                }
            }
        });
    }

    Ok(())
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::url_to_path;
    use tempfile::TempDir;

    #[test]
    fn test_url_to_path_returns_file_for_existing_file_url() {
        let dir = TempDir::new().expect("failed to create temp dir");
        let file_path = dir.path().join("hello world.txt");
        std::fs::write(&file_path, "content").expect("failed to write temp file");

        let path_string = file_path.to_string_lossy();
        let encoded = urlencoding::encode(&path_string);
        let file_url = format!("file://{encoded}");

        let resolved = url_to_path(&file_url);
        assert!(
            resolved.is_some(),
            "existing file URL should resolve to a local path"
        );
    }

    #[test]
    fn test_url_to_path_rejects_invalid_percent_encoded_url() {
        let invalid = "file://%E0%A4%A";
        assert!(
            url_to_path(invalid).is_none(),
            "invalid percent-encoded URL must be rejected"
        );
    }

    #[test]
    fn test_url_to_path_rejects_non_existing_target() {
        let missing_url = "file:///this/path/does/not/exist.txt";
        assert!(
            url_to_path(missing_url).is_none(),
            "non-existing targets must not be returned as openable file paths"
        );
    }
}
