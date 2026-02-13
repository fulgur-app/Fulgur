use std::{fs, sync::Arc};

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Sizable, Theme, ThemeMode, ThemeRegistry, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    notification::NotificationType,
    scroll::ScrollableElement,
    v_flex,
};
use parking_lot::Mutex;

use crate::fulgur::{
    Fulgur,
    ui::themes::{reload_themes_and_update, themes_directory_path},
};

/// Make a select theme item
///
/// ### Arguments
/// - `entity`: The entity
/// - `theme_name`: The name of the theme
/// - `is_current_theme`: Whether the theme is the current theme
/// - `current_theme_shared`: The shared state of the current theme
///
/// ### Returns
/// - `Div`: The select theme item
fn make_select_theme_item(
    entity: Entity<Fulgur>,
    theme_name: String,
    is_current_theme: bool,
    current_theme_shared: Arc<Mutex<String>>,
    cx: &mut App,
) -> Stateful<Div> {
    let id = SharedString::from(format!("Select_{}", theme_name));
    h_flex()
        .id(id)
        .cursor_pointer()
        .justify_between()
        .my_2()
        .border_1()
        .border_color(cx.theme().border)
        .child(div().p_2().text_sm().child(theme_name.clone()))
        .when(is_current_theme, |this| this.bg(cx.theme().muted))
        .on_click(move |_this, _window, cx| {
            if let Some(theme_config) = ThemeRegistry::global(cx)
                .themes()
                .get(theme_name.as_str())
                .cloned()
            {
                Theme::global_mut(cx).apply_config(&theme_config);
                let theme_name_clone = theme_name.clone();
                entity.update(cx, |fulgur, cx| {
                    fulgur.settings.app_settings.theme = theme_name_clone.clone().into();
                    let _ = fulgur.update_and_propagate_settings(cx);
                    *current_theme_shared.lock() = theme_name_clone.clone();
                    cx.notify();
                });
                cx.refresh_windows();
            }
        })
        .hover(|this| this.bg(cx.theme().muted))
}

/// Make a select theme list
///
/// ### Arguments
/// - `entity`: The entity
/// - `themes`: The list of themes
/// - `current_theme`: The name of the current theme
/// - `current_theme_shared`: The shared state of the current theme
///
/// ### Returns
/// - `Div`: The select theme list
fn make_select_theme_list(
    entity: Entity<Fulgur>,
    themes: Vec<String>,
    current_theme: String,
    current_theme_shared: Arc<Mutex<String>>,
    cx: &mut App,
) -> Div {
    let entity = entity.clone();
    div().rounded_md().children(
        themes
            .clone()
            .iter()
            .map(move |theme| {
                make_select_theme_item(
                    entity.clone(),
                    theme.clone(),
                    *theme == current_theme,
                    current_theme_shared.clone(),
                    cx,
                )
            })
            .collect::<Vec<Stateful<Div>>>(),
    )
}

impl Fulgur {
    /// Add a theme to the themes directory. Prompt the user for the path to the theme file.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn add_theme(&self, window: &mut Window, cx: &mut Context<Self>) {
        let path_future = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Select theme".into()),
        });
        let settings = self.settings.clone();
        let entity = cx.entity();
        cx.spawn_in(window, async move |_view, window| {
            let paths = path_future.await.ok()?.ok()??;
            let theme_path = paths.first()?.clone();
            if theme_path.extension().and_then(|s| s.to_str()) != Some("json") {
                window
                    .update(|window, cx| {
                        window.push_notification(
                            "Invalid file type. Please select a JSON theme file.".to_string(),
                            cx,
                        );
                    })
                    .ok()?;
                return None;
            }
            let themes_dir = match themes_directory_path() {
                Ok(path) => path,
                Err(e) => {
                    log::error!("Failed to get themes directory: {}", e);
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from(format!(
                                "Failed to access themes directory: {}",
                                e
                            ));
                            window.push_notification((NotificationType::Error, notification), cx);
                        })
                        .ok()?;
                    return None;
                }
            };
            let filename = match theme_path.file_name() {
                Some(name) => name,
                None => {
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from("Invalid theme file path.");
                            window.push_notification((NotificationType::Error, notification), cx);
                        })
                        .ok()?;
                    return None;
                }
            };
            let dest_path = themes_dir.join(filename);
            match fs::copy(&theme_path, &dest_path) {
                Ok(_) => {
                    log::info!("Theme file copied to: {:?}", dest_path);
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from(format!(
                                "Theme '{}' added successfully!",
                                filename.to_string_lossy()
                            ));
                            window.push_notification((NotificationType::Success, notification), cx);
                            reload_themes_and_update(&settings, entity, cx);
                        })
                        .ok()?;
                }
                Err(e) => {
                    log::error!("Failed to copy theme file: {}", e);
                    window
                        .update(|window, cx| {
                            let notification =
                                SharedString::from(format!("Failed to add theme: {}", e));
                            window.push_notification((NotificationType::Error, notification), cx);
                        })
                        .ok()?;
                }
            }
            Some(())
        })
        .detach();
    }

    /// Open theme selector as a sheet (sliding panel from right side)
    ///
    /// This is an alternative to the dialog-based theme selector
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn select_theme_sheet(&self, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let current_theme = self.settings.app_settings.theme.to_string();
        let current_theme_shared = Arc::new(Mutex::new(current_theme.clone()));
        let viewport_height = window.viewport_size().height;
        window.open_sheet(cx, move |sheet, _window, cx| {
            let themes = ThemeRegistry::global(cx).sorted_themes();
            let light_themes: Vec<String> = themes
                .iter()
                .filter(|theme| theme.mode == ThemeMode::Light)
                .map(|theme| theme.name.to_string())
                .collect();
            let dark_themes: Vec<String> = themes
                .iter()
                .filter(|theme| theme.mode == ThemeMode::Dark)
                .map(|theme| theme.name.to_string())
                .collect();

            let entity_dark = entity.clone();
            let entity_light = entity.clone();
            let current_theme_shared_dark = current_theme_shared.clone();
            let current_theme_shared_light = current_theme_shared.clone();
            let current_theme_display = current_theme_shared.lock().clone();
            let max_height = px((viewport_height - px(150.0)).into()); //TODO: Make this dynamic based on the content
            sheet
                .title("Select Theme")
                .size(px(400.))
                .overlay(false)
                .child(
                    v_flex()
                        .overflow_y_scrollbar()
                        .gap_2()
                        .h(max_height)
                        .child(div().text_lg().child("Dark themes"))
                        .child(make_select_theme_list(
                            entity_dark,
                            dark_themes.clone(),
                            current_theme_display.clone(),
                            current_theme_shared_dark,
                            cx,
                        ))
                        .child(div().text_lg().mt_4().child("Light themes"))
                        .child(make_select_theme_list(
                            entity_light,
                            light_themes.clone(),
                            current_theme_display.clone(),
                            current_theme_shared_light,
                            cx,
                        )),
                )
                .footer({
                    let entity_footer = entity.clone();
                    h_flex()
                        .justify_between()
                        .w_full()
                        .child(
                            Button::new("add-theme-footer")
                                .child("Add new theme...")
                                .small()
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    entity_footer.update(cx, |this, cx| {
                                        this.add_theme(window, cx);
                                    });
                                    // Close sheet after opening add theme dialog
                                    // User can reopen to see new theme
                                    //window.close_sheet(cx);
                                }),
                        )
                        .child(
                            Button::new("ok-footer")
                                .child("OK")
                                .small()
                                .primary()
                                .cursor_pointer()
                                .on_click(|_, window, cx| {
                                    window.close_sheet(cx);
                                }),
                        )
                })
        });
    }
}
