use gpui::prelude::FluentBuilder;
use gpui::{
    App, Context, Div, Entity, InteractiveElement, ParentElement, PathPromptOptions, SharedString,
    Stateful, StatefulInteractiveElement, Styled, Window, div, px,
};
use gpui_component::{
    ActiveTheme, Sizable, Theme, ThemeMode, ThemeRegistry, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    notification::NotificationType,
    scroll::ScrollableElement,
    v_flex,
};
use parking_lot::Mutex;
use std::{fs, sync::Arc};

use crate::fulgur::{
    Fulgur,
    ui::themes::{reload_themes_and_update, themes_directory_path},
};

/// Make a select theme item.
///
/// ### Parameters:
/// - `entity`: The Fulgur entity handle.
/// - `theme_name`: The theme name represented by this item.
/// - `is_current_theme`: Whether this item matches the currently selected theme.
/// - `current_theme_shared`: Shared current-theme value for sheet highlighting.
/// - `cx`: The application context.
///
/// ### Returns:
/// `Stateful<Div>`: Represents one clickable theme row.
fn make_select_theme_item(
    entity: Entity<Fulgur>,
    theme_name: String,
    is_current_theme: bool,
    current_theme_shared: Arc<Mutex<String>>,
    cx: &mut App,
) -> Stateful<Div> {
    let id = SharedString::from(format!("Select_{theme_name}"));
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
            let selected_theme_name = theme_name.clone();
            let should_refresh = entity.update(cx, |fulgur, cx| {
                fulgur.switch_active_theme_from_sheet(
                    cx,
                    selected_theme_name.as_str(),
                    &current_theme_shared,
                )
            });
            if should_refresh {
                cx.refresh_windows();
            }
        })
        .hover(|this| this.bg(cx.theme().muted))
}

/// Check whether a theme is the currently active theme.
///
/// ### Parameters:
/// - `theme_name`: The theme name to evaluate.
/// - `current_theme`: The currently active theme name.
///
/// ### Returns:
/// `true` when both theme names match, otherwise `false`.
fn is_current_theme(theme_name: &str, current_theme: &str) -> bool {
    theme_name == current_theme
}

/// Make the select-theme list.
///
/// ### Parameters:
/// - `entity`: The Fulgur entity handle.
/// - `themes`: The list of available theme names.
/// - `current_theme`: The currently active theme.
/// - `current_theme_shared`: Shared current-theme value for sheet highlighting.
/// - `cx`: The application context.
///
/// ### Returns:
/// `Div`: Represents the rendered theme list.
fn make_select_theme_list(
    entity: &Entity<Fulgur>,
    themes: &[String],
    current_theme: String,
    current_theme_shared: Arc<Mutex<String>>,
    cx: &mut App,
) -> Div {
    let entity = entity.clone();
    div().rounded_md().children(
        themes
            .iter()
            .map(move |theme| {
                make_select_theme_item(
                    entity.clone(),
                    theme.clone(),
                    is_current_theme(theme, &current_theme),
                    current_theme_shared.clone(),
                    cx,
                )
            })
            .collect::<Vec<Stateful<Div>>>(),
    )
}

impl Fulgur {
    /// Apply a theme selected from the select-theme sheet.
    ///
    /// ### Parameters:
    /// - `cx`: The Fulgur context.
    /// - `theme_name`: The selected theme name.
    /// - `current_theme_shared`: Shared current-theme value used by the sheet.
    ///
    /// ### Returns:
    /// - `true`: The theme exists and was applied.
    /// - `false`: The theme was not found in the registry.
    fn switch_active_theme_from_sheet(
        &mut self,
        cx: &mut Context<Self>,
        theme_name: &str,
        current_theme_shared: &Arc<Mutex<String>>,
    ) -> bool {
        if let Some(theme_config) = ThemeRegistry::global(cx).themes().get(theme_name).cloned() {
            Theme::global_mut(cx).apply_config(&theme_config);
            self.settings.app_settings.theme = theme_name.to_string().into();
            if let Err(error) = self.update_and_propagate_settings(cx) {
                log::error!(
                    "Failed to propagate settings after theme '{theme_name}' selection: {error}"
                );
            }
            *current_theme_shared.lock() = theme_name.to_string();
            cx.notify();
            true
        } else {
            false
        }
    }

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
                    log::error!("Failed to get themes directory: {e}");
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from(format!(
                                "Failed to access themes directory: {e}"
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
                    log::info!("Theme file copied to: {}", dest_path.display());
                    window
                        .update(|window, cx| {
                            let notification = SharedString::from(format!(
                                "Theme '{}' added successfully!",
                                filename.to_string_lossy()
                            ));
                            window.push_notification((NotificationType::Success, notification), cx);
                            reload_themes_and_update(&settings, &entity, cx);
                        })
                        .ok()?;
                }
                Err(e) => {
                    log::error!("Failed to copy theme file: {e}");
                    window
                        .update(|window, cx| {
                            let notification =
                                SharedString::from(format!("Failed to add theme: {e}"));
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
                            &entity_dark,
                            &dark_themes,
                            current_theme_display.clone(),
                            current_theme_shared_dark,
                            cx,
                        ))
                        .child(div().text_lg().mt_4().child("Light themes"))
                        .child(make_select_theme_list(
                            &entity_light,
                            &light_themes,
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

#[cfg(test)]
mod tests {
    #[cfg(feature = "gpui-test-support")]
    use super::Fulgur;
    use super::is_current_theme;
    #[cfg(feature = "gpui-test-support")]
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use core::prelude::v1::test;
    #[cfg(feature = "gpui-test-support")]
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    #[cfg(feature = "gpui-test-support")]
    use gpui_component::ThemeRegistry;
    #[cfg(feature = "gpui-test-support")]
    use parking_lot::Mutex;
    #[cfg(feature = "gpui-test-support")]
    use std::{cell::RefCell, rc::Rc, sync::Arc};

    #[test]
    fn test_is_current_theme_matches_expected_value() {
        assert!(is_current_theme("Tokyo Night", "Tokyo Night"));
        assert!(!is_current_theme("Tokyo Night", "Solarized"));
    }

    #[cfg(feature = "gpui-test-support")]
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(gpui_component::init);
        cx.update(|cx| {
            cx.set_global(SharedAppState::new(
                Settings::new(),
                Arc::new(Mutex::new(Vec::new())),
            ));
            cx.set_global(WindowManager::new());
        });

        let fulgur_slot: Rc<RefCell<Option<Entity<Fulgur>>>> = Rc::new(RefCell::new(None));
        let slot = Rc::clone(&fulgur_slot);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let fulgur = fulgur_slot
            .borrow_mut()
            .take()
            .expect("expected fulgur entity");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        (fulgur, visual_cx)
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_switch_active_theme_from_sheet_updates_current_theme(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let current_theme = fulgur.read_with(&visual_cx, |this, _| {
            this.settings.app_settings.theme.to_string()
        });

        let current_theme_for_lookup = current_theme.clone();
        let target_theme = visual_cx.update(|_window, cx| {
            let themes = ThemeRegistry::global(cx).sorted_themes();
            themes
                .iter()
                .map(|theme| theme.name.to_string())
                .find(|name| name != &current_theme_for_lookup)
                .or_else(|| themes.first().map(|theme| theme.name.to_string()))
                .expect("expected at least one registered theme")
        });

        let current_theme_shared = Arc::new(Mutex::new(current_theme.clone()));
        let target_theme_for_apply = target_theme.clone();
        let current_theme_shared_for_apply = current_theme_shared.clone();
        let applied = visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.switch_active_theme_from_sheet(
                    cx,
                    target_theme_for_apply.as_str(),
                    &current_theme_shared_for_apply,
                )
            })
        });

        assert!(applied);

        let switched_theme = fulgur.read_with(&visual_cx, |this, _| {
            this.settings.app_settings.theme.to_string()
        });
        assert_eq!(switched_theme, target_theme);
        assert_eq!(*current_theme_shared.lock(), target_theme);
    }

    #[cfg(feature = "gpui-test-support")]
    #[gpui::test]
    fn test_switch_active_theme_from_sheet_is_noop_for_unknown_theme(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let initial_theme = fulgur.read_with(&visual_cx, |this, _| {
            this.settings.app_settings.theme.to_string()
        });
        let current_theme_shared = Arc::new(Mutex::new(initial_theme.clone()));
        let current_theme_shared_for_apply = current_theme_shared.clone();

        let applied = visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                this.switch_active_theme_from_sheet(
                    cx,
                    "__missing_theme_for_test__",
                    &current_theme_shared_for_apply,
                )
            })
        });

        assert!(!applied);

        let final_theme = fulgur.read_with(&visual_cx, |this, _| {
            this.settings.app_settings.theme.to_string()
        });
        assert_eq!(final_theme, initial_theme);
        assert_eq!(*current_theme_shared.lock(), initial_theme);
    }
}
