use anyhow::anyhow;
use gpui::*;
use gpui_component::{ThemeRegistry, Theme, TitleBar, Root};
use rust_embed::RustEmbed;
use std::borrow::Cow;

use crate::lightspeed::{Lightspeed, NewFile, OpenFile, CloseFile, SwitchTheme, CloseAllFiles};
mod lightspeed;
// Asset loader for icons
#[derive(RustEmbed)]
#[folder = "./assets"]
#[include = "icons/**/*.svg"]
pub struct Assets;

impl AssetSource for Assets {
    fn load(&self, path: &str) -> Result<Option<Cow<'static, [u8]>>> {
        if path.is_empty() {
            return Ok(None);
        }

        Self::get(path)
            .map(|f| Some(f.data))
            .ok_or_else(|| anyhow!("could not find asset at path \"{path}\""))
    }

    fn list(&self, path: &str) -> Result<Vec<SharedString>> {
        Ok(Self::iter()
            .filter_map(|p| p.starts_with(path).then(|| p.into()))
            .collect())
    }
}

fn main() {
    let app = Application::new().with_assets(Assets);

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        Lightspeed::init(cx);
        
        // Set up keyboard shortcuts
        cx.bind_keys([
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-o", OpenFile, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-o", OpenFile, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-n", NewFile, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-n", NewFile, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-w", CloseFile, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-w", CloseFile, None),
            #[cfg(target_os = "macos")]
            KeyBinding::new("cmd-shift-w", CloseAllFiles, None),
            #[cfg(not(target_os = "macos"))]
            KeyBinding::new("ctrl-shift-w", CloseAllFiles, None),
        ]);
            
        // Handle theme switching from menu
        cx.on_action(|switch: &SwitchTheme, cx| {
            let theme_name = switch.0.clone();
            if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(&theme_name).cloned() {
                Theme::global_mut(cx).apply_config(&theme_config);
            }
            cx.refresh_windows();
        });

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                // Enable custom title bar
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                window.set_window_title("Lightspeed");
                let view = Lightspeed::new(window, cx);
                // Focus the view so keyboard shortcuts work immediately
                view.focus_handle(cx).focus(window);
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view.into(), window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}