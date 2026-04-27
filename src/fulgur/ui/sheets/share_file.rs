use crate::fulgur::{
    Fulgur,
    sync::{
        share::{Device, ShareFileRequest, get_devices, get_icon, share_file},
        synchronization::SynchronizationStatus,
    },
    ui::{
        icons::CustomIcon,
        notifications::progress::{CancelCallback, start_progress},
    },
};
use gpui::{
    App, Div, Element, Entity, SharedString, StatefulInteractiveElement, prelude::FluentBuilder,
};
use gpui::{Context, InteractiveElement, ParentElement, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    notification::NotificationType,
    scroll::ScrollableElement,
    v_flex,
};
use std::sync::Arc;

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
    let has_public_key = device.public_key.is_some();
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
        .when(has_public_key, |this| this.cursor_pointer())
        .when(!has_public_key, |this| this.opacity(0.5))
        .when(has_public_key, |this| {
            this.hover(|hover| hover.bg(cx.theme().muted))
        })
        .when(is_selected && has_public_key, |this| {
            this.bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
        })
        .child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .justify_start()
                        .gap_2()
                        .child(get_icon(device_for_icon))
                        .child(div().child(device_name))
                        .child(
                            div()
                                .text_xs()
                                .child(format!("Expires: {}", device_expires)),
                        ),
                )
                .when(!has_public_key, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("No public key for this device"),
                    )
                }),
        )
        .when(is_selected && has_public_key, |this| {
            this.child(Icon::new(CustomIcon::Zap))
        })
        .when(has_public_key, |this| {
            this.on_click(move |_event, _window, _cx| {
                let mut ids = selected_ids.lock();
                if let Some(pos) = ids.iter().position(|id| id == &device_id) {
                    ids.remove(pos);
                } else {
                    ids.push(device_id.clone());
                }
            })
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
/// Extracts file content on the UI thread, then spawns a background thread
/// for compression, encryption, and upload.
///
/// ### Arguments
/// - `selected_ids`: The selected device IDs
/// - `devices`: The list of all devices (with their public keys)
/// - `entity`: The Fulgur entity
/// - `window`: The window context
/// - `cx`: The application context
fn handle_share_file(
    selected_ids: Arc<parking_lot::Mutex<Vec<String>>>,
    devices: Arc<Vec<Device>>,
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
    let (
        sync_settings,
        request,
        token_state,
        http_agent,
        pending_notification,
        max_file_size_bytes,
    ) = entity.update(cx, |this, cx| {
        let active_tab = this.get_active_editor_tab();
        let content: Arc<str> = active_tab
            .as_ref()
            .map(|tab| Arc::from(tab.content.read(cx).value().as_str()))
            .unwrap_or_else(|| Arc::from(""));
        let file_path = active_tab.as_ref().and_then(|tab| tab.file_path().cloned());
        let file_name = file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_string();
        (
            this.settings.app_settings.synchronization_settings.clone(),
            ShareFileRequest {
                content,
                file_name,
                device_ids: ids,
                file_path,
            },
            Arc::clone(&this.shared_state(cx).sync_state.token_state),
            Arc::clone(&this.shared_state(cx).http_agent),
            this.shared_state(cx)
                .sync_state
                .pending_notification
                .clone(),
            this.shared_state(cx)
                .sync_state
                .max_file_size_bytes
                .load(std::sync::atomic::Ordering::Acquire),
        )
    });
    window.close_sheet(cx);
    let progress_label = format!("Sharing {}...", request.file_name);
    let cancel_callback: Option<CancelCallback> = Some(Box::new(|_, _| {}));
    let progress = start_progress(window, cx, progress_label.into(), cancel_callback);
    let cancel_flag = progress.cancel_flag();
    std::thread::spawn(move || {
        let _progress = progress;
        let result = share_file(
            &sync_settings,
            request,
            &devices,
            token_state,
            &http_agent,
            max_file_size_bytes,
        );
        if cancel_flag.load(std::sync::atomic::Ordering::Acquire) {
            // User cancelled — drop the result silently.
            return;
        }
        let notification = match result {
            Ok(share_result) => {
                if share_result.is_complete_success() {
                    (
                        NotificationType::Success,
                        SharedString::from(share_result.summary_message()),
                    )
                } else if share_result.successes.is_empty() {
                    (
                        NotificationType::Error,
                        SharedString::from(share_result.summary_message()),
                    )
                } else {
                    (
                        NotificationType::Warning,
                        SharedString::from(share_result.summary_message()),
                    )
                }
            }
            Err(e) => {
                log::error!("Failed to share file: {}", e);
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to share file: {}", e)),
                )
            }
        };
        *pending_notification.lock() = Some(notification);
    });
}

impl Fulgur {
    /// Open the share file sheet
    ///
    /// Initiates a background fetch for devices (and reconnection if needed).
    /// The sheet opens once the device list is available, processed in the render loop
    /// via `process_pending_share_sheet`.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn open_share_file_sheet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        if !self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            log::warn!("Synchronization is not activated");
            return;
        }
        let shared = self.shared_state(cx);
        if shared.sync_state.connection_status.lock().is_connecting() {
            log::debug!("Already connecting, ignoring share button click");
            return;
        }
        let needs_reconnect = !self.is_connected(cx);
        if needs_reconnect {
            log::info!("Not connected to sync server, attempting reconnection...");
        }
        let shared = self.shared_state(cx);
        crate::fulgur::sync::synchronization::set_sync_server_connection_status(
            shared.sync_state.connection_status.clone(),
            SynchronizationStatus::Connecting,
        );
        *shared.sync_state.connecting_since.lock() = Some(std::time::Instant::now());
        let synchronization_settings = self.settings.app_settings.synchronization_settings.clone();
        let token_state = Arc::clone(&shared.sync_state.token_state);
        let http_agent = Arc::clone(&shared.http_agent);
        let connection_status = shared.sync_state.connection_status.clone();
        let connecting_since = shared.sync_state.connecting_since.clone();
        let device_name_shared = shared.sync_state.device_name.clone();
        let pending_shared_files = shared.sync_state.pending_shared_files.clone();
        let pending_devices = shared.sync_state.pending_devices.clone();
        let max_file_size_bytes = shared.sync_state.max_file_size_bytes.clone();
        self.pending_share_sheet = true;
        std::thread::spawn(move || {
            if needs_reconnect {
                match crate::fulgur::sync::synchronization::initial_synchronization(
                    &synchronization_settings,
                    Arc::clone(&token_state),
                    &http_agent,
                ) {
                    Ok(begin_response) => {
                        *device_name_shared.lock() = Some(begin_response.device_name.clone());
                        *pending_shared_files.lock() = begin_response.shares;
                        crate::fulgur::sync::synchronization::store_server_max_file_size(
                            &max_file_size_bytes,
                            begin_response.max_file_size_bytes,
                        );
                        crate::fulgur::sync::synchronization::set_sync_server_connection_status(
                            connection_status.clone(),
                            SynchronizationStatus::Connected,
                        );
                    }
                    Err(e) => {
                        let status = SynchronizationStatus::from_error(&e);
                        crate::fulgur::sync::synchronization::set_sync_server_connection_status(
                            connection_status,
                            status,
                        );
                        *connecting_since.lock() = None;
                        *pending_devices.lock() = Some((Err(format!("{}", e)), false));
                        return;
                    }
                }
            }
            let result = get_devices(&synchronization_settings, token_state, &http_agent);
            *connecting_since.lock() = None;
            match result {
                Ok((devices, server_max_size)) => {
                    crate::fulgur::sync::synchronization::store_server_max_file_size(
                        &max_file_size_bytes,
                        server_max_size,
                    );
                    crate::fulgur::sync::synchronization::set_sync_server_connection_status(
                        connection_status,
                        SynchronizationStatus::Connected,
                    );
                    *pending_devices.lock() = Some((Ok(devices), needs_reconnect));
                }
                Err(e) => {
                    *pending_devices.lock() = Some((Err(format!("{}", e)), false));
                }
            }
        });
    }

    /// Process pending share sheet data from background device fetch
    ///
    /// Called in the render loop to check if the background device fetch has completed,
    /// and opens the share sheet if devices are available.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn process_pending_share_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.pending_share_sheet {
            return;
        }
        let pending = self
            .shared_state(cx)
            .sync_state
            .pending_devices
            .lock()
            .take();
        let (result, needs_sse_restart) = match pending {
            Some(pending) => pending,
            None => return,
        };
        self.pending_share_sheet = false;
        if needs_sse_restart {
            self.restart_sse_connection(cx);
        }
        match result {
            Ok(devices) => {
                self.show_share_sheet(devices, window, cx);
            }
            Err(e) => {
                log::error!("Failed to prepare share: {}", e);
                let status = *self.shared_state(cx).sync_state.connection_status.lock();
                let dialog_message = match status {
                    SynchronizationStatus::AuthenticationFailed => {
                        "Authentication failed. Check your e-mail and device API key in the synchronization settings."
                    }
                    SynchronizationStatus::ConnectionFailed => {
                        "Connection failed. Check the server URL in the synchronization settings."
                    }
                    _ => {
                        "An error occurred while connecting to the sync server. Check your synchronization settings."
                    }
                };
                window.open_alert_dialog(cx, move |dialog, _, _| dialog.child(dialog_message));
            }
        }
    }

    /// Show the share file sheet with the given devices
    ///
    /// ### Arguments
    /// - `devices`: The list of devices to display
    /// - `window`: The window context
    /// - `cx`: The application context
    fn show_share_sheet(
        &mut self,
        devices: Vec<Device>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if devices.is_empty() {
            window.push_notification(
                (
                    NotificationType::Warning,
                    SharedString::from("No devices available for sharing."),
                ),
                cx,
            );
            return;
        }
        let entity = cx.entity();
        let selected_ids: Arc<parking_lot::Mutex<Vec<String>>> =
            Arc::new(parking_lot::Mutex::new(Vec::new()));
        let devices: Arc<Vec<Device>> = Arc::new(devices);
        let viewport_height = window.viewport_size().height;
        window.open_sheet(cx, move |sheet, _window, cx| {
            #[cfg(target_os = "linux")]
            let sheet_overhead = px(200.0);
            #[cfg(not(target_os = "linux"))]
            let sheet_overhead = px(150.0);
            let max_height = px((viewport_height - sheet_overhead).into());
            let selected_ids_for_list = selected_ids.clone();
            let selected_ids_for_ok = selected_ids.clone();
            let entity_for_ok = entity.clone();
            let devices_ref = devices.clone();
            let devices_button = devices.clone();
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
                                        devices_button.clone(),
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{Device, Fulgur};
    use crate::fulgur::{
        settings::Settings, shared_state::SharedAppState, window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    /// Initialize globals and open a test window with a Root-mounted `Fulgur`.
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    fn make_device(id: &str) -> Device {
        Device {
            id: id.to_string(),
            name: format!("{id}-name"),
            device_type: "desktop".to_string(),
            public_key: Some("age1dummypublickey".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[gpui::test]
    fn test_process_pending_share_sheet_ignores_queue_when_sheet_not_pending(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_share_sheet = false;
                *this.shared_state(cx).sync_state.pending_devices.lock() =
                    Some((Err("device fetch failed".to_string()), false));
                this.process_pending_share_sheet(window, cx);
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_devices
                        .lock()
                        .is_some(),
                    "queue must be untouched when there is no pending share sheet"
                );
            });
        });
    }

    #[gpui::test]
    fn test_process_pending_share_sheet_keeps_waiting_until_background_result_arrives(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_share_sheet = true;
                *this.shared_state(cx).sync_state.pending_devices.lock() = None;
                this.process_pending_share_sheet(window, cx);
                assert!(
                    this.pending_share_sheet,
                    "sheet should stay pending while background task has not produced a result"
                );
            });
        });
    }

    #[gpui::test]
    fn test_process_pending_share_sheet_consumes_error_result_and_clears_flag(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_share_sheet = true;
                *this.shared_state(cx).sync_state.pending_devices.lock() =
                    Some((Err("authentication failed".to_string()), false));
                this.process_pending_share_sheet(window, cx);
                assert!(
                    !this.pending_share_sheet,
                    "pending flag must be cleared once a result is consumed"
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_devices
                        .lock()
                        .is_none(),
                    "pending result must be drained from shared state"
                );
            });
        });
    }

    #[gpui::test]
    fn test_process_pending_share_sheet_consumes_success_result_and_clears_flag(
        cx: &mut TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.pending_share_sheet = true;
                *this.shared_state(cx).sync_state.pending_devices.lock() =
                    Some((Ok(vec![make_device("device-1")]), false));
                this.process_pending_share_sheet(window, cx);
                assert!(
                    !this.pending_share_sheet,
                    "pending flag must be cleared when device list is consumed"
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_devices
                        .lock()
                        .is_none(),
                    "device queue must be drained after processing"
                );
            });
        });
    }
}
