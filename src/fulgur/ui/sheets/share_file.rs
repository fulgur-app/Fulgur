use std::sync::Arc;

use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    notification::NotificationType,
    scroll::ScrollableElement,
    v_flex,
};

use crate::fulgur::{
    Fulgur,
    sync::{
        share::{Device, ShareFilePayload, get_devices, get_icon, share_file},
        sync::SynchronizationStatus,
    },
    ui::icons::CustomIcon,
};

/// Create a device item for the share file sheet
///
/// ### Arguments
/// - `device`: The device to display
/// - `is_selected`: Whether the device is selected
/// - `selected_ids`: The shared state of selected device IDs
/// - `idx`: The index of the device (for unique ID)
/// - `cx`: The application context
///
/// ### Returns
/// - `impl Element`: The device item element (a Div, actually)
fn make_device_item(
    device: &Device,
    is_selected: bool,
    selected_ids: Arc<parking_lot::Mutex<Vec<String>>>,
    idx: usize,
    cx: &App,
) -> impl Element {
    let device_id = device.id.clone();
    let device_name = device.name.clone();
    let device_expires = device.expires_at.clone();
    let device_for_icon = device;

    h_flex()
        .id(("share-device-sheet", idx))
        .items_center()
        .justify_between()
        .w_full()
        .p_2()
        .my_2()
        .rounded_sm()
        .border_color(cx.theme().border)
        .border_1()
        .cursor_pointer()
        .hover(|this| this.bg(cx.theme().muted))
        .when(is_selected, |this| {
            this.bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
        })
        .child(
            h_flex()
                .items_center()
                .justify_start()
                .gap_2()
                .child(get_icon(&device_for_icon))
                .child(div().child(device_name))
                .child(
                    div()
                        .text_xs()
                        .child(format!("Expires: {}", device_expires)),
                ),
        )
        .when(is_selected, |this| this.child(Icon::new(CustomIcon::Zap)))
        .on_click(move |_event, _window, _cx| {
            let mut ids = selected_ids.lock();
            if let Some(pos) = ids.iter().position(|id| id == &device_id) {
                ids.remove(pos);
            } else {
                ids.push(device_id.clone());
            }
        })
}

/// Create the device list for the share file sheet
///
/// ### Arguments
/// - `devices`: The list of devices (wrapped in Arc)
/// - `selected_ids`: The shared state of selected device IDs
/// - `cx`: The application context
///
/// ### Returns
/// - `Div`: The list of devices
fn make_device_list(
    devices: Arc<Vec<Device>>,
    selected_ids: Arc<parking_lot::Mutex<Vec<String>>>,
    cx: &App,
) -> Div {
    div().gap_2().children(
        devices
            .iter()
            .enumerate()
            .map(|(idx, device)| {
                let is_selected = selected_ids.lock().contains(&device.id);
                make_device_item(device, is_selected, selected_ids.clone(), idx, cx)
            })
            .collect::<Vec<_>>(),
    )
}

/// Handle the share file action when OK is clicked
///
/// ### Arguments
/// - `selected_ids`: The selected device IDs
/// - `entity`: The Fulgur entity
/// - `window`: The window context
/// - `cx`: The application context
fn handle_share_file(
    selected_ids: Arc<parking_lot::Mutex<Vec<String>>>,
    entity: Entity<Fulgur>,
    window: &mut Window,
    cx: &mut App,
) {
    let ids = selected_ids.lock().clone();
    if ids.is_empty() {
        window.push_notification(
            (
                NotificationType::Warning,
                SharedString::from("Please select at least one device to share with."),
            ),
            cx,
        );
        return;
    }
    let result = entity.update(cx, |this, cx| {
        let content = this
            .get_active_editor_tab()
            .map(|tab| tab.content.read(cx).value().to_string())
            .unwrap_or_default();
        let file_name = this
            .get_active_editor_tab()
            .and_then(|tab| tab.file_path.as_ref())
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_string();
        let payload = ShareFilePayload {
            content,
            file_name,
            device_ids: ids,
        };
        share_file(
            &this.settings.app_settings.synchronization_settings,
            payload,
            Arc::clone(&this.shared_state(cx).token_state),
        )
    });
    match result {
        Ok(expiration_date) => {
            let notification = (
                NotificationType::Success,
                SharedString::from(format!(
                    "File shared successfully until {}",
                    expiration_date
                )),
            );
            window.push_notification(notification, cx);
            window.close_sheet(cx);
        }
        Err(e) => {
            log::error!("Failed to share file: {}", e.to_string());
            let notification = (
                NotificationType::Error,
                SharedString::from(format!("Failed to share file: {}", e.to_string())),
            );
            window.push_notification(notification, cx);
        }
    }
}

impl Fulgur {
    /// Open the share file sheet
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn open_share_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            log::warn!("Synchronization is not activated");
            return;
        }
        if !self.is_connected(cx) {
            log::info!("Not connected to sync server, attempting reconnection...");
            window.push_notification(
                (
                    NotificationType::Info,
                    SharedString::from("Attempting to connect to sync server..."),
                ),
                cx,
            );
            let synchronization_settings =
                self.settings.app_settings.synchronization_settings.clone();
            let token_state = Arc::clone(&self.shared_state(cx).token_state);
            let result = crate::fulgur::sync::sync::initial_synchronization(
                &synchronization_settings,
                token_state,
            );
            match result {
                Ok(begin_response) => {
                    {
                        let mut key = self.shared_state(cx).encryption_key.lock();
                        *key = Some(begin_response.encryption_key);
                    }
                    {
                        let mut device_name = self.shared_state(cx).device_name.lock();
                        *device_name = Some(begin_response.device_name.clone());
                    }
                    {
                        let mut files = self.shared_state(cx).pending_shared_files.lock();
                        *files = begin_response.shares;
                    }
                    *self.shared_state(cx).sync_server_connection_status.lock() =
                        SynchronizationStatus::Connected;
                    self.restart_sse_connection(cx);
                }
                Err(e) => {
                    let connection_status = SynchronizationStatus::from_error(&e);
                    *self.shared_state(cx).sync_server_connection_status.lock() = connection_status;
                    let dialog_message = match connection_status {
                        SynchronizationStatus::AuthenticationFailed => {
                            "Authentication failed. Check your e-mail and device API key in the synchronization settings."
                        }
                        SynchronizationStatus::ConnectionFailed => {
                            "Connection failed. Check the server URL in the synchronization settings."
                        }
                        SynchronizationStatus::Other => {
                            "An error occurred while connecting to the sync server. Check your synchronization settings."
                        }
                        SynchronizationStatus::NotActivated => {
                            "Synchronization is not activated. You can activate synchronization in the settings."
                        }
                        SynchronizationStatus::Disconnected => {
                            "Could not connect to the sync server. Check your synchronization settings."
                        }
                        SynchronizationStatus::Connected => "Unknown error occurred.",
                    };
                    window
                        .open_dialog(cx, move |dialog, _, _| dialog.alert().child(dialog_message));
                    return;
                }
            }
        }
        let synchronization_settings = self.settings.app_settings.synchronization_settings.clone();
        let devices = get_devices(
            &synchronization_settings,
            Arc::clone(&self.shared_state(cx).token_state),
        );
        let devices = match devices {
            Ok(devices) => devices,
            Err(e) => {
                log::error!("Failed to get devices: {}", e.to_string());
                window.push_notification(
                    (
                        NotificationType::Error,
                        SharedString::from(format!("Failed to get devices: {}", e.to_string())),
                    ),
                    cx,
                );
                return;
            }
        };
        let entity = cx.entity();
        let selected_ids: Arc<parking_lot::Mutex<Vec<String>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let devices: Arc<Vec<Device>> = Arc::new(devices);
        let viewport_height = window.viewport_size().height;
        window.open_sheet(cx, move |sheet, _window, cx| {
            let max_height = px((viewport_height - px(150.0)).into());
            let selected_ids_for_list = selected_ids.clone();
            let selected_ids_for_ok = selected_ids.clone();
            let entity_for_ok = entity.clone();
            let devices_ref = devices.clone();
            sheet
                .title("Share with...")
                .size(px(400.))
                .overlay(true)
                .child(
                    v_flex()
                        .overflow_y_scrollbar()
                        .gap_2()
                        .h(max_height)
                        .child(make_device_list(devices_ref, selected_ids_for_list, cx)),
                )
                .footer(
                    h_flex()
                        .justify_end()
                        .w_full()
                        .gap_2()
                        .child(
                            Button::new("cancel-share")
                                .child("Cancel")
                                .small()
                                .cursor_pointer()
                                .on_click(|_, window, cx| {
                                    window.close_sheet(cx);
                                }),
                        )
                        .child(
                            Button::new("ok-share")
                                .child("Share")
                                .small()
                                .primary()
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    handle_share_file(
                                        selected_ids_for_ok.clone(),
                                        entity_for_ok.clone(),
                                        window,
                                        cx,
                                    );
                                }),
                        ),
                )
        });
    }
}
