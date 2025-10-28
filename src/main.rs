use gpui::*;
use gpui_component::{dropdown::*, *};
mod themes;

pub struct HelloWorld {
    themes_dropdown: Entity<DropdownState<Vec<SharedString>>>,
}

impl HelloWorld {
    fn new(window: &mut Window, cx: &mut App) -> Entity<Self> {
        let themes = ThemeRegistry::global(cx)
            .sorted_themes()
            .iter()
            .map(|theme| theme.name.clone())
            .collect::<Vec<SharedString>>();
        
        let themes_dropdown = cx.new(|cx| {
            DropdownState::new(themes, Some(IndexPath::default()), window, cx)
        });

        cx.new(|cx| {
            cx.subscribe_in(&themes_dropdown, window, Self::on_theme_change)
                .detach();

            Self { themes_dropdown }
        })
    }

    fn on_theme_change(
        &mut self,
        _dropdown: &Entity<DropdownState<Vec<SharedString>>>,
        event: &DropdownEvent<Vec<SharedString>>,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let DropdownEvent::Confirm(Some(theme_name)) = event {
            if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
                Theme::global_mut(cx).apply_config(&theme_config);
                cx.refresh_windows();
            }
        }
    }
}

impl Render for HelloWorld {
    fn render(&mut self, _window: &mut Window, _cx: &mut Context<Self>) -> impl IntoElement {
        div()
            .v_flex()
            .gap_2()
            .size_full()
            .items_center()
            .justify_center()
            .child("Hello, World!")
            .child(div().w_128().child(
                Dropdown::new(&self.themes_dropdown)
                    .placeholder("Select theme...")
                    .title_prefix("Theme: ")
                    .cleanable()
                    .into_any_element()
            ))
    }
}

fn main() {
    let app = Application::new();

    app.run(move |cx| {
        // This must be called before using any GPUI Component features.
        gpui_component::init(cx);
        themes::init(cx);

        cx.spawn(async move |cx| {
            cx.open_window(WindowOptions::default(), |window, cx| {
                window.set_window_title("Lightspeed");
                let view = HelloWorld::new(window, cx);
                // This first level on the window, should be a Root.
                cx.new(|cx| Root::new(view.into(), window, cx))
            })?;

            Ok::<_, anyhow::Error>(())
        })
        .detach();
    });
}