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

fn main() {
    let app = Application::new().with_assets(Assets);
    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        fulgur::Fulgur::init(cx);
        cx.spawn(async move |cx| {
            let window_options = WindowOptions {
                titlebar: Some(TitleBar::title_bar_options()),
                ..Default::default()
            };
            cx.open_window(window_options, |window, cx| {
                window.set_window_title("Fulgur");
                let view = fulgur::Fulgur::new(window, cx);
                let view_clone = view.clone();
                window.on_window_should_close(cx, move |window, cx| {
                    view_clone.update(cx, |fulgur, cx| {
                        if fulgur.settings.app_settings.confirm_exit {
                            fulgur.quit(window, cx);
                            false
                        } else {
                            if let Err(e) = fulgur.save_state(cx) {
                                eprintln!("Failed to save app state: {}", e);
                            }
                            true
                        }
                    })
                });
                view.read(cx).focus_active_tab(window, cx);
                cx.new(|cx| Root::new(view, window, cx))
            })?;
            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}
