#![cfg_attr(target_os = "windows", windows_subsystem = "windows")]

use gpui::*;
use gpui_component::{Root, TitleBar};
use rust_embed::RustEmbed;
use std::borrow::Cow;

mod fulgur;

#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
#[include = "icon_square.png"]
#[include = "icon.png"]
#[include = "icon.icns"]
#[include = "icon.ico"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }
        if let Some(data) = Self::get(path) {
            return Ok(Some(data.data));
        }
        let path_without_prefix = path.strip_prefix("assets/").unwrap_or(path);
        if path_without_prefix != path {
            if let Some(data) = Self::get(path_without_prefix) {
                return Ok(Some(data.data));
            }
        }
        Ok(None)
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

// Convert a file:// URL string to a PathBuf
// @param url_string: The URL string to convert (e.g., "file:///Users/user/file.txt")
// @return: The PathBuf if successful, None otherwise
fn url_to_path(url_string: &str) -> Option<std::path::PathBuf> {
    log::debug!("Converting URL to path: {}", url_string);
    let path_str = url_string.strip_prefix("file://").unwrap_or(url_string);
    match urlencoding::decode(path_str) {
        Ok(decoded) => {
            let path = std::path::PathBuf::from(decoded.into_owned());
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
    if let Err(e) = fulgur::logger::init() {
        log::error!("Failed to initialize logger: {}", e);
    }
    let current_version = env!("CARGO_PKG_VERSION");
    log::info!("=== Fulgur Text Editor v{} Starting ===", current_version);
    log::info!("Platform: {}", std::env::consts::OS);
    log::info!("Architecture: {}", std::env::consts::ARCH);
    let args: Vec<String> = std::env::args().collect();
    log::info!("Command-line arguments: {:?}", args);
    if args.len() > 1 {
        log::info!("File to open from command-line: {}", args[1]);
    }
    let cli_file_paths: Vec<std::path::PathBuf> = args
        .iter()
        .skip(1)
        .filter_map(|arg| {
            let path = std::path::PathBuf::from(arg);
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
    let pending_files: std::sync::Arc<std::sync::Mutex<Vec<std::path::PathBuf>>> =
        std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let pending_files_clone = pending_files.clone();
    app.on_open_urls(move |urls| {
        log::debug!("Received {} file URL(s) from macOS open event", urls.len());
        for url in &urls {
            log::debug!("macOS open URL: {}", url);
        }
        let file_paths: Vec<std::path::PathBuf> =
            urls.iter().filter_map(|url| url_to_path(url)).collect();

        if file_paths.is_empty() {
            log::warn!("No valid file paths from macOS open event");
            return;
        }
        log::debug!(
            "Processing {} valid file(s) from macOS open event",
            file_paths.len()
        );
        if let Ok(mut pending) = pending_files_clone.lock() {
            pending.extend(file_paths);
            log::debug!(
                "Added files to pending queue, total pending: {}",
                pending.len()
            );
        } else {
            log::error!("Failed to lock pending files queue");
        }
    });
    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        fulgur::Fulgur::init(cx);
        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitleBar::title_bar_options()),
                // IMPORTANT: window_decorations is ONLY set on Linux!
                // Windows and macOS use the default (None)
                #[cfg(target_os = "linux")]
                window_decorations: Some(gpui::WindowDecorations::Client),
                ..Default::default()
            };
            let window = cx.open_window(window_options, |window, cx| {
                window.set_window_title("Fulgur");
                let view = fulgur::Fulgur::new(window, cx, pending_files.clone());
                let view_clone = view.clone();

                window.on_window_should_close(cx, move |window, cx| {
                    view_clone.update(cx, |fulgur, cx| {
                        if fulgur.settings.app_settings.confirm_exit {
                            fulgur.quit(window, cx);
                            false
                        } else {
                            if let Err(e) = fulgur.save_state(cx) {
                                log::error!("Failed to save app state: {}", e);
                            }
                            true
                        }
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

                cx.new(|cx| Root::new(view, window, cx))
            })?;
            window
                .update(cx, |_, window, _| {
                    window.activate_window();
                })
                .expect("failed to update window");
            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
