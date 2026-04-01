use gpui::{App, Entity, IntoElement, ParentElement, SharedString, Styled, div};
use gpui_component::{
    ActiveTheme, Sizable,
    button::{Button, ButtonVariants},
    h_flex,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
    v_flex,
};

use crate::fulgur::{
    Fulgur, settings::AppSettings, sync::synchronization::perform_initial_synchronization,
    utils::crypto_helper,
};

const DEVICE_KEY_PLACEHOLDER: &str = "<Device Key>";

/// Create the Application settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
///
/// ### Returns
/// - `SettingPage`: The Application settings page
pub fn create_application_page(entity: Entity<Fulgur>) -> SettingPage {
    let default_app_settings = AppSettings::new();

    SettingPage::new("Application")
        .default_open(true)
        .groups(vec![
            SettingGroup::new().title("General").items(vec![
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
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.confirm_exit),
                )
                .description("Show confirmation dialog before exiting the application."),
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
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.debug_mode),
                )
                .description("Enables debug mode, showing more info in the logs."),
            ]),
            SettingGroup::new().title("Synchronization").items({
                let mut sync_items = vec![];
                sync_items.push(SettingItem::render({
                    move |_options, _window, cx| {
                        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
                        if let Some(error_msg) = shared.sync_error.lock().as_ref() {
                            v_flex()
                                .w_full()
                                .p_3()
                                .mb_2()
                                .bg(cx.theme().muted)
                                .border_1()
                                .border_color(cx.theme().border)
                                .rounded(gpui::px(4.0))
                                .child(
                                    div()
                                        .text_color(cx.theme().foreground)
                                        .text_size(gpui::px(13.0))
                                        .font_weight(gpui::FontWeight::SEMIBOLD)
                                        .child(error_msg.clone()),
                                )
                                .into_any_element()
                        } else {
                            div().into_any_element()
                        }
                    }
                }));
                sync_items.push(SettingItem::new(
                    "Activate Synchronization",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .app_settings
                                    .synchronization_settings
                                    .is_synchronization_activated
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .is_synchronization_activated = val;
                                        if val && let Err(e) =
                                                crypto_helper::check_private_public_keys(&mut this.settings)
                                        {
                                                log::error!("Failed to check private/public keys: {}", e);
                                        }
                                        if let Err(e) = this.update_and_propagate_settings(cx) {
                                            log::error!("Failed to save settings: {}", e);
                                        }
                                    });
                                if val {
                                    perform_initial_synchronization(entity.clone(), cx);
                                }
                            }
                        },
                    )
                    .default_value(default_app_settings.synchronization_settings.is_synchronization_activated),
                )
                .description("Activate synchronization with the server and saves the relevant keys in the system's keychain."));
                sync_items.push(SettingItem::new(
                    "Deduplication",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .app_settings
                                    .synchronization_settings
                                    .is_deduplication
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings
                                        .app_settings
                                        .synchronization_settings
                                        .is_deduplication = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.synchronization_settings.is_deduplication),
                )
                .description("Avoid duplicate shares of the same file on the server."));
                sync_items.push(SettingItem::new(
                    "Server URL",
                    SettingField::input(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .app_settings
                                    .synchronization_settings
                                    .server_url
                                    .as_ref()
                                    .map(|s| SharedString::from(s.clone()))
                                    .unwrap_or_default()
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: SharedString, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    let url = if val.is_empty() {
                                        None
                                    } else {
                                        Some(val.to_string())
                                    };
                                    this.settings
                                        .app_settings
                                        .synchronization_settings
                                        .server_url = url;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                    this.restart_sse_connection(cx);
                                });
                            }
                        },
                    )
                    .default_value(
                        default_app_settings
                            .synchronization_settings
                            .server_url
                            .clone()
                            .map(SharedString::from)
                            .unwrap_or_default(),
                    ),
                )
                .description("URL of the synchronization server."));
                sync_items.push(SettingItem::new(
                    "Email",
                    SettingField::input(
                        {
                            let entity = entity.clone();
                            move |cx: &App| {
                                entity
                                    .read(cx)
                                    .settings
                                    .app_settings
                                    .synchronization_settings
                                    .email
                                    .as_ref()
                                    .map(|s| SharedString::from(s.clone()))
                                    .unwrap_or_default()
                            }
                        },
                        {
                            let entity = entity.clone();
                            move |val: SharedString, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    let email = if val.is_empty() {
                                        None
                                    } else {
                                        Some(val.to_string())
                                    };
                                    this.settings.app_settings.synchronization_settings.email =
                                        email;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {}", e);
                                    }
                                    this.restart_sse_connection(cx);
                                });
                            }
                        },
                    )
                    .default_value(
                        default_app_settings
                            .synchronization_settings
                            .email
                            .clone()
                            .map(SharedString::from)
                            .unwrap_or_default(),
                    ),
                )
                .description("Email for synchronization."));
                sync_items.push(SettingItem::new(
                    "Device Key",
                    SettingField::input(
                        move |_cx: &App| SharedString::from(DEVICE_KEY_PLACEHOLDER),
                        {
                            let entity = entity.clone();
                            move |val: SharedString, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    let key = if val.is_empty() {
                                        None
                                    } else if val == DEVICE_KEY_PLACEHOLDER {
                                        return;
                                    } else {
                                        Some(val.to_string())
                                    };
                                    if let Err(e) =
                                        crypto_helper::save_device_api_key_to_keychain(key)
                                    {
                                        log::error!("Failed to save device API key: {}", e);
                                    } else {
                                        log::info!("Device API key saved successfully");
                                        // Clear cached token to force re-authentication with new device key
                                        this.shared_state(cx).sync_state.token_state.clear_token();
                                    }
                                    this.restart_sse_connection(cx);
                                });
                            }
                        },
                    ),
                )
                .description(
                    "Device Key for synchronization (stored in keychain).",
                ));
                sync_items.push(SettingItem::render({
                    let entity = entity.clone();
                    move |_options, _window, cx| {
                        let is_connecting = cx
                            .global::<crate::fulgur::shared_state::SharedAppState>()
                            .sync_state
                            .connection_status
                            .lock()
                            .is_connecting();
                        let label = if is_connecting {
                            "Connecting..."
                        } else {
                            "Begin Synchronization"
                        };
                        h_flex()
                            .w_full()
                            .justify_end()
                            .mt_2()
                            .child({
                                let mut btn = Button::new("begin-synchronization-button")
                                    .label(label)
                                    .primary()
                                    .small()
                                    .loading(is_connecting);
                                if !is_connecting {
                                    btn = btn.cursor_pointer().on_click({
                                        let entity = entity.clone();
                                        move |_, _window, cx| {
                                            let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
                                            shared.sync_state.token_state.clear_token();
                                            perform_initial_synchronization(
                                                entity.clone(),
                                                cx,
                                            );
                                        }
                                    });
                                }
                                btn
                            })
                            .into_any_element()
                    }
                }));
                sync_items
            }),
        ])
}
