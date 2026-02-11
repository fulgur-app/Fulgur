#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::*;
// gpui_component is used in create_window function
use parking_lot::Mutex;
use rust_embed::RustEmbed;
use std::{borrow::Cow, path::PathBuf, sync::Arc};

use fulgur::fulgur;

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
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
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
    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
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
    log::debug!("Converting URL to path: {}", url_string);
    let path_str = url_string.strip_prefix("file://").unwrap_or(url_string);
    match urlencoding::decode(path_str) {
        Ok(decoded) => {
            let path = PathBuf::from(decoded.into_owned());
            if path.exists() && path.is_file() {
                log::debug!("Converted URL to valid file path: {:?}", path);
                Some(path)
            } else {
                log::warn!("URL converted to path but file doesn't exist: {:?}", path);
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
    if let Err(e) = fulgur::utils::logger::init() {
        log::error!("Failed to initialize logger: {}", e);
    }
    let current_version = env!("CARGO_PKG_VERSION");
    log::info!("=== Fulgur v{} Starting ===", current_version);
    log::info!("Platform: {}", std::env::consts::OS);
    log::info!("Architecture: {}", std::env::consts::ARCH);
    let args: Vec<String> = std::env::args().collect();
    log::info!("Command-line arguments: {:?}", args);
    if args.len() > 1 {
        log::info!("File to open from command-line: {}", args[1]);
    }
    let cli_file_paths: Vec<PathBuf> = args
        .iter()
        .skip(1)
        .filter_map(|arg| {
            let path = PathBuf::from(arg);
            if path.exists() && path.is_file() {
                log::debug!("Valid file argument found: {:?}", path);
                Some(path)
            } else {
                if !arg.is_empty() {
                    log::warn!("Invalid or non-existent file argument: {:?}", arg);
                }
                None
            }
        })
        .collect();

    let app = Application::new().with_assets(Assets);
    let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
    let pending_files_clone = pending_files.clone();
    app.on_open_urls(move |urls| {
        log::debug!("Received {} file URL(s) from macOS open event", urls.len());
        for url in &urls {
            log::debug!("macOS open URL: {}", url);
        }
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
        fulgur::Fulgur::init(cx);
        let shared_state = fulgur::shared_state::SharedAppState::new(pending_files.clone());
        cx.set_global(shared_state);
        cx.set_global(fulgur::window_manager::WindowManager::new());
        let windows_state = fulgur::state_persistence::WindowsState::load().ok();
        let num_saved_windows = windows_state
            .as_ref()
            .map(|ws| ws.windows.len())
            .unwrap_or(0);
        if num_saved_windows > 0 {
            log::info!("Restoring {} saved window(s)", num_saved_windows);
        } else {
            log::info!("No saved state, creating initial window");
        }
        if let Some(ws) = windows_state {
            for (index, _window_state) in ws.windows.iter().enumerate() {
                let cli_files = if index == 0 {
                    cli_file_paths.clone()
                } else {
                    vec![]
                };
                let window_index = index;

                cx.spawn(async move |cx| create_window(cx, window_index, cli_files).await)
                    .detach();
            }
        } else {
            cx.spawn(async move |cx| create_window(cx, 0, cli_file_paths).await)
                .detach();
        }
    });
}

/// Create a new window
///
/// ### Arguments
/// * `cx` - The application context
/// * `window_index` - The index of the window to create
/// * `cli_file_paths` - The paths of the files to open in the window
async fn create_window(
    cx: &mut gpui::AsyncApp,
    window_index: usize,
    cli_file_paths: Vec<std::path::PathBuf>,
) -> anyhow::Result<()> {
    let (window_bounds, saved_display_id) =
        if let Ok(windows_state) = fulgur::state_persistence::WindowsState::load() {
            if let Some(window_state) = windows_state.windows.get(window_index) {
                (
                    Some(window_state.window_bounds.to_gpui_bounds()),
                    window_state.window_bounds.display_id,
                )
            } else {
                (None, None)
            }
        } else {
            (None, None)
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
        })?
    } else {
        None
    };
    let window_options = gpui::WindowOptions {
        titlebar: Some(gpui_component::TitleBar::title_bar_options()),
        window_bounds,
        display_id,
        #[cfg(target_os = "linux")]
        window_decorations: Some(gpui::WindowDecorations::Client),
        ..Default::default()
    };
    let window = cx.open_window(window_options, |window, cx| {
        window.set_window_title("Fulgur");
        let view = fulgur::Fulgur::new(window, cx, window_index);
        let window_handle = window.window_handle();
        let window_id = window_handle.window_id();
        view.update(cx, |fulgur, _cx| {
            fulgur.window_id = window_id;
        });
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
            view.read(cx).focus_active_tab(window, cx);
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
        })?;
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
