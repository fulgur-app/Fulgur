use crate::fulgur::{Fulgur, settings::AppSettings};
use gpui::{App, Entity};
use gpui_component::setting::{SettingField, SettingGroup, SettingItem, SettingPage};

mod general;
mod profile_status;
mod profiles_table;
mod sync_controls;

use general::render_tab_color_style_select;
#[cfg(target_os = "macos")]
use general::render_title_bar_style_select;
use profiles_table::render_profiles_table;
use sync_controls::{render_add_server_button, render_master_switch, render_sync_error_banner};

/// Create the Application settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
///
/// ### Returns
/// - `SettingPage`: The Application settings page
pub fn create_application_page(entity: &Entity<Fulgur>) -> SettingPage {
    let default_app_settings = AppSettings::new();

    let mut general_items = vec![
                SettingItem::new(
                    "Confirm Exit",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.app_settings.confirm_exit
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.app_settings.confirm_exit = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {e}");
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.confirm_exit),
                )
                .description("Show confirmation dialog before exiting the application."),
                SettingItem::new(
                    "Persist Unsaved Changes",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .app_settings
                                    .persist_unsaved_buffers
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.app_settings.persist_unsaved_buffers = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {e}");
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.persist_unsaved_buffers),
                )
                .description(
                    "Restore unsaved edits after a restart. When off, only file paths are kept and untitled tabs are discarded.",
                ),
                SettingItem::new(
                    "Debug mode",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.app_settings.debug_mode
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.app_settings.debug_mode = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {e}");
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.debug_mode),
                )
                .description("Enables debug mode, showing more info in the logs."),
                SettingItem::new(
                    "Tab Color Style",
                    SettingField::render({
                        let entity = entity.clone();
                        move |_options, _window, cx: &mut App| {
                            render_tab_color_style_select(&entity, cx)
                        }
                    }),
                )
                .description("How a tab's color tag is shown: title text or a dot."),
    ];
    general_items.extend(title_bar_style_items(entity));

    SettingPage::new("Application")
        .default_open(true)
        .groups(vec![
            SettingGroup::new().title("General").items(general_items),
            SettingGroup::new().title("Synchronization").items(vec![
                render_sync_error_banner(),
                render_master_switch(entity),
                render_profiles_table(entity),
                render_add_server_button(entity),
            ]),
        ])
}

/// Build the title bar style setting item, which only exists on macOS.
///
/// ### Arguments
/// - `entity`: The Fulgur entity the field reads the current style from and writes back to
///
/// ### Returns
/// - `Vec<SettingItem>`: The single chooser on macOS, empty on every other platform
fn title_bar_style_items(entity: &Entity<Fulgur>) -> Vec<SettingItem> {
    #[cfg(target_os = "macos")]
    {
        let entity = entity.clone();
        vec![
            SettingItem::new(
                "Title Bar Style",
                SettingField::render(move |_options, _window, cx: &mut App| {
                    render_title_bar_style_select(&entity, cx)
                }),
            )
            .description(
                "Keep a separate title bar above the tab bar, or merge the tabs into the title bar.",
            ),
        ]
    }
    #[cfg(not(target_os = "macos"))]
    {
        let _ = entity;
        Vec::new()
    }
}
