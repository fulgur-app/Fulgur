use std::ops::DerefMut;
use std::sync::Arc;

use crate::fulgur::{
    Fulgur, editor_tab,
    sync::{
        share::{Device, ShareFilePayload, get_devices, get_icon, share_file},
        sync::SynchronizationStatus,
    },
    ui::{
        components_utils::{EMPTY, UTF_8},
        icons::CustomIcon,
        languages::{self, SupportedLanguage},
    },
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, WindowExt, h_flex,
    input::{Input, InputState, Position},
    notification::NotificationType,
    scroll::ScrollableElement,
    v_flex,
};

/// State for device selection dialog
struct DeviceSelectionState {
    devices: Vec<Device>,
    selected_ids: Vec<String>,
}

impl DeviceSelectionState {
    /// Create a new device selection state
    ///
    /// ### Arguments
    /// - `devices`: The devices to select from
    ///
    /// ### Returns
    /// - `DeviceSelectionState`: The new device selection state
    fn new(devices: Vec<Device>) -> Self {
        Self {
            devices,
            selected_ids: Vec::new(),
        }
    }

    /// Toggle the selection of a device
    ///
    /// ### Arguments
    /// - `device_id`: The ID of the device
    fn toggle_selection(&mut self, device_id: &str) {
        if let Some(pos) = self.selected_ids.iter().position(|id| id == device_id) {
            self.selected_ids.remove(pos);
        } else {
            self.selected_ids.push(device_id.to_string());
        }
    }

    /// Check if a device is selected
    ///
    /// ### Arguments
    /// - `device_id`: The ID of the device
    ///
    /// ### Returns
    /// - `True`: If the device is selected, `False` otherwise
    fn is_selected(&self, device_id: &str) -> bool {
        self.selected_ids.contains(&device_id.to_string())
    }
}

impl Render for DeviceSelectionState {
    /// Render the device selection state
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered device selection state
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let window_height = window.window_bounds().get_bounds().size.height;
        let max_height = window_height - px(200.0); //TODO: Make this dynamic
        v_flex()
            .w_full()
            .max_h(max_height)
            .overflow_y_scrollbar()
            .children(
                self.devices
                    .iter()
                    .enumerate()
                    .map(|(idx, device)| {
                        let device_id = device.id.clone();
                        let is_selected = self.is_selected(&device_id);
                        h_flex()
                            .id(("share-device", idx))
                            .items_center()
                            .justify_between()
                            .w_full()
                            .p_2()
                            .my_2()
                            .rounded_sm()
                            .border_color(cx.theme().border)
                            .border_1()
                            .hover(|this| this.bg(cx.theme().muted))
                            .when(is_selected, |this| {
                                this.bg(cx.theme().accent)
                                    .text_color(cx.theme().accent_foreground)
                            })
                            .cursor_pointer()
                            .child(
                                h_flex()
                                    .items_center()
                                    .justify_start()
                                    .gap_2()
                                    .child(get_icon(&device))
                                    .child(div().child(device.name.clone()))
                                    .child(
                                        div().text_xs().child(format!(
                                            "Expires: {}",
                                            device.expires_at.clone()
                                        )),
                                    ),
                            )
                            .when(is_selected, |this| this.child(Icon::new(CustomIcon::Zap)))
                            .on_click(cx.listener(move |this, _event: &ClickEvent, _window, cx| {
                                this.toggle_selection(&device_id);
                                cx.notify();
                            }))
                    })
                    .collect::<Vec<_>>(),
            )
    }
}

/// Create a status bar item
///
/// ### Arguments
/// - `content`: The content of the status bar item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `Div`: A status bar item
pub fn status_bar_item_factory(content: impl IntoElement, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

/// Create a status bar button
///
/// ### Arguments
/// - `content`: The content of the status bar button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
///
/// ### Returns
/// - `Div`: A status bar button
pub fn status_bar_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
) -> Div {
    status_bar_item_factory(content, border_color)
        .hover(|this| this.bg(accent_color))
        .cursor_pointer()
}

/// Create a status bar item, right hand side
///
/// ### Arguments
/// - `content`: The content of the status bar right item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `impl IntoElement`: A status bar right item
pub fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_l_1()
}

/// Create a status bar toggle button
///
/// ### Arguments
/// - `content`: The content of the status bar toggle button
/// - `border_color`: The color of the border
/// - `accent_color`: The color of the accent
/// - `checked`: Whether the toggle is checked
///
/// ### Returns
/// - `Div`: A status bar toggle button
pub fn status_bar_toggle_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
    checked: bool,
) -> Div {
    let mut button = status_bar_button_factory(content, border_color, accent_color);
    if checked {
        button = button.bg(accent_color);
    }
    button
}

/// Create a status bar sync button
///
/// ### Arguments
/// - `connected_icon`: The icon to display when connected
/// - `disconnected_icon`: The icon to display when disconnected
/// - `border_color`: The color of the border
/// - `connected_color`: The color of the connected button
/// - `connected_foreground_color`: The foreground color of the connected button
/// - `connected_hover_color`: The hover color of the connected button
/// - `disconnected_color`: The color of the disconnected button
/// - `disconnected_foreground_color`: The foreground color of the disconnected button
/// - `disconnected_hover_color`: The hover color of the disconnected button
/// - `is_connected`: Whether the device is connected
///
/// ### Returns
/// - `Div`: A status bar sync button
pub fn status_bar_sync_button(
    connected_icon: Icon,
    disconnected_icon: Icon,
    border_color: Hsla,
    connected_color: Hsla,
    connected_foreground_color: Hsla,
    connected_hover_color: Hsla,
    disconnected_color: Hsla,
    disconnected_foreground_color: Hsla,
    disconnected_hover_color: Hsla,
    is_connected: bool,
) -> Div {
    let mut button = div()
        .text_sm()
        .flex()
        .items_center()
        .justify_center()
        .px_4()
        .py_1()
        .border_color(border_color)
        .cursor_pointer();
    if is_connected {
        button = button
            .child(connected_icon)
            .bg(connected_color)
            .text_color(connected_foreground_color)
            .hover(|this| this.bg(connected_hover_color));
    } else {
        button = button
            .child(disconnected_icon)
            .bg(disconnected_color)
            .text_color(disconnected_foreground_color)
            .hover(|this| this.bg(disconnected_hover_color));
    }
    button
}

/// Create a status bar left item
///
/// ### Arguments
/// - `content`: The content of the status bar left item
/// - `border_color`: The color of the border
///
/// ### Returns
/// - `impl IntoElement`: A status bar left item
#[allow(dead_code)]
pub fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_r_1()
}

/// Handle the click on OK button in the jump to line dialog
///
/// ### Arguments
/// - `jump_to_line_input`: The input state entity
/// - `entity`: The Fulgur entity
/// - `cx`: The application context
///
/// ### Returns
/// - `true` if the jump to line is successful, `false` otherwise
fn handle_jump_to_line_ok(
    jump_to_line_input: Entity<InputState>,
    entity: Entity<Fulgur>,
    cx: &mut App,
) -> bool {
    let text = jump_to_line_input.read(cx).value();
    let text_shared = SharedString::from(text);
    let jump = editor_tab::extract_line_number(text_shared);
    entity.update(cx, |this, cx| {
        if let Ok(jump) = jump {
            this.pending_jump = Some(jump);
            this.jump_to_line_dialog_open = false;
            cx.notify();
            true
        } else {
            this.pending_jump = None;
            false
        }
    });
    false
}

/// Handle share file dialog OK action
///
/// ### Arguments
/// - `state`: The device selection state
/// - `entity`: The Fulgur entity
/// - `window`: The window context
/// - `cx`: The application context
///
/// ### Returns
/// - `true` if the file is shared successfully, `false` otherwise
fn handle_share_file_ok(
    state: Entity<DeviceSelectionState>,
    entity: Entity<Fulgur>,
    window: &mut Window,
    cx: &mut App,
) -> bool {
    let selected_ids = state.read(cx).selected_ids.clone();
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
            device_ids: selected_ids,
        };
        share_file(
            &this.settings.app_settings.synchronization_settings,
            payload,
            Arc::clone(&this.token_state),
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
        }
        Err(e) => {
            log::error!("Failed to share file: {}", e.to_string());
            let notification = (
                NotificationType::Error,
                SharedString::from(format!("Failed to share file: {}", e.to_string())),
            );
            window.push_notification(notification, cx);
            return false;
        }
    }
    true
}

/// Handle sync button click - opens share dialog or shows error
///
/// ### Arguments
/// - `instance`: The Fulgur instance
/// - `window`: The window context
/// - `cx`: The application context
fn handle_sync_button_click(instance: &mut Fulgur, window: &mut Window, cx: &mut Context<Fulgur>) {
    if !instance
        .settings
        .app_settings
        .synchronization_settings
        .is_synchronization_activated
    {
        log::warn!("Synchronization is not activated");
        return;
    }
    if !instance.is_connected() {
        log::info!("Not connected to sync server, attempting reconnection...");
        window.push_notification(
            (
                NotificationType::Info,
                SharedString::from("Attempting to connect to sync server..."),
            ),
            cx,
        );
        let synchronization_settings = instance
            .settings
            .app_settings
            .synchronization_settings
            .clone();
        let token_state = Arc::clone(&instance.token_state);
        let result = crate::fulgur::sync::sync::initial_synchronization(
            &synchronization_settings,
            token_state,
        );
        match result {
            Ok(begin_response) => {
                {
                    let mut key = instance.encryption_key.lock();
                    *key = Some(begin_response.encryption_key);
                }
                {
                    let mut device_name = instance.device_name.lock();
                    *device_name = Some(begin_response.device_name.clone());
                }
                {
                    let mut files = instance.pending_shared_files.lock();
                    *files = begin_response.shares;
                }
                *instance.sync_server_connection_status.lock() = SynchronizationStatus::Connected;
                // window.push_notification(
                //     (
                //         NotificationType::Success,
                //         SharedString::from("Connected to sync server successfully"),
                //     ),
                //     cx,
                // );
                instance.restart_sse_connection();
            }
            Err(e) => {
                let connection_status = SynchronizationStatus::from_error(&e);
                *instance.sync_server_connection_status.lock() = connection_status;
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
                    SynchronizationStatus::Connected => {
                        // Should not happen
                        "Unknown error occurred."
                    }
                };
                window.open_dialog(cx, move |dialog, _, _| dialog.alert().child(dialog_message));
                return;
            }
        }
    }
    let synchronization_settings = instance
        .settings
        .app_settings
        .synchronization_settings
        .clone();
    let devices = get_devices(&synchronization_settings, Arc::clone(&instance.token_state));
    let devices = match devices {
        Ok(devices) => devices,
        Err(e) => {
            log::error!("Failed to get devices: {}", e.to_string());
            return;
        }
    };
    let entity = cx.entity();
    let state = cx.new(|_cx| DeviceSelectionState::new(devices));
    window.open_dialog(cx.deref_mut(), move |modal, _window, _cx| {
        let state_clone_for_ok = state.clone();
        let entity_clone = entity.clone();
        modal
            .confirm()
            .title("Share with...")
            .child(state.clone())
            .overlay_closable(true)
            .close_button(false)
            .on_ok(move |_event: &ClickEvent, window, cx| {
                handle_share_file_ok(state_clone_for_ok.clone(), entity_clone.clone(), window, cx)
            })
    });
}

/// Jump to line
///
/// ### Arguments
/// - `instance`: The Fulgur instance
/// - `window`: The window context
/// - `cx`: The application context
pub fn jump_to_line(instance: &mut Fulgur, window: &mut Window, cx: &mut Context<Fulgur>) {
    let jump_to_line_input = instance.jump_to_line_input.clone();
    jump_to_line_input.update(cx, |input_state, cx| {
        input_state.set_value("", window, cx);
        cx.notify();
    });
    let entity = cx.entity().clone();
    instance.jump_to_line_dialog_open = true;
    window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
        let focus_handle = jump_to_line_input.read(cx).focus_handle(cx);
        window.focus(&focus_handle);
        let jump_to_line_input_clone = jump_to_line_input.clone();
        let entity_clone = entity.clone();
        modal
            .confirm()
            .keyboard(true)
            .child(Input::new(&jump_to_line_input))
            .overlay_closable(true)
            .close_button(false)
            .on_ok(move |_event: &ClickEvent, _window, cx| {
                handle_jump_to_line_ok(jump_to_line_input_clone.clone(), entity_clone.clone(), cx)
            })
    });
}

impl Fulgur {
    pub fn jump_to_line(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        jump_to_line(self, window, cx);
    }

    /// Render the status bar
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `impl IntoElement`: The rendered status bar element
    pub fn render_status_bar(&self, cx: &mut Context<Self>) -> impl IntoElement {
        let (cursor_pos, language) = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    (
                        editor_tab.content.read(cx).cursor_position(),
                        Some(editor_tab.language),
                    )
                } else {
                    (Position::default(), Some(SupportedLanguage::Plain))
                }
            }
            None => (Position::default(), None),
        };
        let language = match language {
            Some(language) => languages::pretty_name(&language),
            None => EMPTY.to_string(),
        };
        let encoding = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    editor_tab.encoding.clone()
                } else {
                    EMPTY.to_string()
                }
            }
            None => UTF_8.to_string(),
        };
        let jump_to_line_button_content = format!(
            "Ln {}, Col {}",
            cursor_pos.line + 1,
            cursor_pos.character + 1
        );
        let jump_to_line_button = status_bar_button_factory(
            jump_to_line_button_content,
            cx.theme().border,
            cx.theme().muted,
        );
        let jump_to_line_button = jump_to_line_button.on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event: &MouseDownEvent, window, cx| {
                jump_to_line(this, window, cx);
            }),
        );
        let language_button =
            status_bar_button_factory(language, cx.theme().border, cx.theme().muted).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    //set_language(this, window, cx, language_shared.clone());
                    this.render_select_language_sheet(window, cx);
                }),
            );
        let active_editor_tab = self.get_active_editor_tab();
        let show_markdown_preview = active_editor_tab.unwrap().show_markdown_preview; //TODO: Handle the case where there is no active editor tab even if it shouldn't happen
        let preview_button = status_bar_toggle_button_factory(
            "Preview".to_string(),
            cx.theme().border,
            cx.theme().muted,
            show_markdown_preview,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                let active_editor_tab = this.get_active_editor_tab_mut();
                if let Some(active_editor_tab) = active_editor_tab {
                    active_editor_tab.show_markdown_preview =
                        !active_editor_tab.show_markdown_preview;
                }
                cx.notify();
            }),
        );
        let show_markdown_toolbar = active_editor_tab.unwrap().show_markdown_toolbar; //TODO: Handle the case where there is no active editor tab even if it shouldn't happen
        let toolbar_button = status_bar_toggle_button_factory(
            "Toolbar".to_string(),
            cx.theme().border,
            cx.theme().muted,
            show_markdown_toolbar,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, _window, cx| {
                let active_editor_tab = this.get_active_editor_tab_mut();
                if let Some(active_editor_tab) = active_editor_tab {
                    active_editor_tab.show_markdown_toolbar =
                        !active_editor_tab.show_markdown_toolbar;
                }
                cx.notify();
            }),
        );
        let is_markdown = self.is_markdown();
        let is_connected = self.is_connected();
        let sync_button = status_bar_sync_button(
            Icon::new(CustomIcon::Zap),
            Icon::new(CustomIcon::ZapOff),
            cx.theme().border,
            cx.theme().primary,
            cx.theme().primary_foreground,
            cx.theme().primary_hover,
            cx.theme().danger,
            cx.theme().danger_foreground,
            cx.theme().danger_hover,
            is_connected,
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(|this, _event, window, cx| {
                handle_sync_button_click(this, window, cx);
            }),
        );
        h_flex()
            .justify_between()
            .bg(cx.theme().tab_bar)
            .py_0()
            .my_0()
            .border_t_1()
            .border_color(cx.theme().border)
            .text_color(cx.theme().foreground)
            .child(
                div()
                    .flex()
                    .justify_start()
                    .when(
                        self.settings
                            .app_settings
                            .synchronization_settings
                            .is_synchronization_activated,
                        |this| this.child(sync_button),
                    )
                    .child(language_button)
                    .when(is_markdown, |this| this.child(preview_button))
                    .when(is_markdown, |this| this.child(toolbar_button)),
            )
            .child(
                div()
                    .flex()
                    .justify_end()
                    .child(jump_to_line_button)
                    .child(status_bar_right_item_factory(encoding, cx.theme().border)),
            )
    }
}
