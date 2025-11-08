use anyhow::anyhow;
use gpui::*;
use gpui_component::{Root, TitleBar};
use rust_embed::RustEmbed;
use std::borrow::Cow;

mod fulgur;

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
        fulgur::Fulgur::init(cx);

        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                // Enable custom title bar
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            };

            cx.open_window(window_options, |window, cx| {
                window.set_window_title("Fulgur");
                let view = fulgur::Fulgur::new(window, cx);

                // Register window close handler to show confirmation modal
                let view_clone = view.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    view_clone.update(cx, |fulgur, cx| {
                        // Check if confirmation is needed
                        if fulgur.settings.app_settings.confirm_exit {
                            // Show modal and prevent closing
                            fulgur.quit(window, cx);
                            false // Prevent window from closing (modal will handle it)
                        } else {
                            // Save state and allow closing
                            if let Err(e) = fulgur.save_state(cx) {
                                eprintln!("Failed to save app state: {}", e);
                            }
                            true // Allow window to close
                        }
                    })
                });

                // Focus the initial tab's content so keyboard shortcuts work immediately
                view.read(cx).focus_active_tab(window, cx);
                // Root must be the window's root component for modals to work
                cx.new(|cx| Root::new(view.into(), window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
