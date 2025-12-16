use std::ops::DerefMut;

use crate::fulgur::{
    Fulgur,
    components_utils::{EMPTY, UTF_8},
    editor_tab,
    icons::CustomIcon,
    languages,
    sync::{Device, ShareFilePayload, get_devices, get_icon, share_file},
};
use gpui::{prelude::FluentBuilder, *};
use gpui_component::{
    ActiveTheme, Icon, WindowExt, h_flex,
    highlighter::Language,
    input::{Input, Position},
    notification::NotificationType,
    scroll::ScrollableElement,
    select::Select,
    v_flex,
};

// State for device selection dialog
struct DeviceSelectionState {
    devices: Vec<Device>,
    selected_ids: Vec<String>,
}

impl DeviceSelectionState {
    // Create a new device selection state
    // @param devices: The devices to select from
    // @return: The new device selection state
    fn new(devices: Vec<Device>) -> Self {
        Self {
            devices,
            selected_ids: Vec::new(),
        }
    }

    // Toggle the selection of a device
    // @param device_id: The ID of the device
    // @return: The new selection state
    fn toggle_selection(&mut self, device_id: &str) {
        if let Some(pos) = self.selected_ids.iter().position(|id| id == device_id) {
            self.selected_ids.remove(pos);
        } else {
            self.selected_ids.push(device_id.to_string());
        }
    }

    // Check if a device is selected
    // @param device_id: The ID of the device
    // @return: True if the device is selected, false otherwise
    fn is_selected(&self, device_id: &str) -> bool {
        self.selected_ids.contains(&device_id.to_string())
    }
}

impl Render for DeviceSelectionState {
    // Render the device selection state
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered device selection state
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

// Create a status bar item
// @param content: The content of the status bar item
// @param border_color: The color of the border
// @return: A status bar item
pub fn status_bar_item_factory(content: impl IntoElement, border_color: Hsla) -> Div {
    div()
        .text_xs()
        .px_2()
        .py_1()
        .border_color(border_color)
        .child(content)
}

// Create a status bar button
// @param content: The content of the status bar button
// @param border_color: The color of the border
// @param accent_color: The color of the accent
// @return: A status bar button
pub fn status_bar_button_factory(
    content: impl IntoElement,
    border_color: Hsla,
    accent_color: Hsla,
) -> Div {
    status_bar_item_factory(content, border_color)
        .hover(|this| this.bg(accent_color))
        .cursor_pointer()
}

// Create a status bar right item
// @param content: The content of the status bar right item
// @param border_color: The color of the border
// @return: A status bar right item
pub fn status_bar_right_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
    status_bar_item_factory(content, border_color) //.border_l_1()
}

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

// Create a status bar left item
// @param content: The content of the status bar left item
// @param border_color: The color of the border
// @return: A status bar left item
// pub fn status_bar_left_item_factory(content: String, border_color: Hsla) -> impl IntoElement {
//     status_bar_item_factory(content, border_color) //.border_r_1()
// }

impl Fulgur {
    // Jump to line
    // @param window: The window context
    // @param cx: The application context
    pub fn jump_to_line(self: &mut Fulgur, window: &mut Window, cx: &mut Context<Self>) {
        let jump_to_line_input = self.jump_to_line_input.clone();
        jump_to_line_input.update(cx, |input_state, cx| {
            input_state.set_value("", window, cx);
            cx.notify();
        });
        let entity = cx.entity().clone();
        self.jump_to_line_dialog_open = true;
        window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = jump_to_line_input.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            let entity_clone = entity.clone();
            let jump_to_line_input_clone = jump_to_line_input.clone();
            modal
                .confirm()
                .keyboard(true)
                .child(Input::new(&jump_to_line_input))
                .overlay_closable(true)
                .close_button(false)
                .on_ok(move |_event: &ClickEvent, _window, cx| {
                    let text = jump_to_line_input_clone.read(cx).value();
                    let text_shared = SharedString::from(text);
                    let jump = editor_tab::extract_line_number(text_shared);
                    let entity_ok = entity_clone.clone();
                    entity_ok.update(cx, |this, cx| {
                        if let Ok(jump) = jump {
                            this.pending_jump = Some(jump);
                            this.jump_to_line_dialog_open = false;
                            cx.notify();
                            return true;
                        } else {
                            this.pending_jump = None;
                            return false;
                        }
                    });
                    false
                })
        });
        return;
    }

    // Set the language via a dialog
    // @param window: The window context
    // @param cx: The application context
    // @param current_language: The current language
    fn set_language(
        self: &mut Fulgur,
        window: &mut Window,
        cx: &mut Context<Self>,
        current_language: SharedString,
    ) {
        let language_dropdown = self.language_dropdown.clone();
        language_dropdown.update(cx, |select_state, cx| {
            select_state.set_selected_value(&current_language, window, cx);
            cx.notify();
        });
        let entity = cx.entity().clone();
        window.open_dialog(cx.deref_mut(), move |modal, window, cx| {
            let focus_handle = language_dropdown.read(cx).focus_handle(cx);
            window.focus(&focus_handle);
            let entity_clone = entity.clone();
            modal
                .confirm()
                .keyboard(true)
                .child(Select::new(&language_dropdown))
                .overlay_closable(true)
                .close_button(false)
                .on_ok({
                    let value = language_dropdown.clone();
                    let entity_ok = entity_clone.clone();
                    move |_event: &ClickEvent, window, cx| {
                        let language_name = value.read(cx).selected_value();
                        if let Some(language_name) = language_name {
                            let language = languages::language_from_pretty_name(&language_name);
                            entity_ok.update(cx, |this, cx| {
                                if let Some(index) = this.active_tab_index {
                                    if let Some(tab) = this.tabs.get_mut(index) {
                                        if let Some(editor_tab) = tab.as_editor_mut() {
                                            editor_tab.force_language(
                                                window,
                                                cx,
                                                language,
                                                &this.settings.editor_settings,
                                            );
                                        }
                                    }
                                }
                            });
                        }
                        true
                    }
                })
        });
    }

    // Render the status bar
    // @param window: The window context
    // @param cx: The application context
    // @return: The rendered status bar element
    pub(super) fn render_status_bar(
        &self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> impl IntoElement {
        let (cursor_pos, language) = match self.active_tab_index {
            Some(index) => {
                if let Some(editor_tab) = self.tabs[index].as_editor() {
                    (
                        editor_tab.content.read(cx).cursor_position(),
                        Some(editor_tab.language),
                    )
                } else {
                    (Position::default(), Some(Language::Plain))
                }
            }
            None => (Position::default(), None),
        };
        let language = match language {
            Some(language) => languages::pretty_name(language),
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
                this.jump_to_line(window, cx);
            }),
        );
        let language_shared = SharedString::from(language.clone());
        let language_button =
            status_bar_button_factory(language, cx.theme().border, cx.theme().muted).on_mouse_down(
                MouseButton::Left,
                cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                    this.set_language(window, cx, language_shared.clone());
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
            self.is_connected.load(std::sync::atomic::Ordering::Relaxed),
        )
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _event: &MouseDownEvent, window, cx| {
                let devices = get_devices(
                    this.settings
                        .app_settings
                        .synchronization_settings
                        .server_url
                        .clone(),
                    this.settings
                        .app_settings
                        .synchronization_settings
                        .email
                        .clone(),
                    this.settings
                        .app_settings
                        .synchronization_settings
                        .key
                        .clone(),
                );
                let devices = match devices {
                    Ok(devices) => devices,
                    Err(e) => {
                        log::error!("Failed to get devices: {}", e);
                        return;
                    }
                };
                let entity = cx.entity();
                let state = cx.new(|_cx| DeviceSelectionState::new(devices));
                window.open_dialog(cx.deref_mut(), move |modal, _window, _cx| {
                    modal
                        .confirm()
                        .title("Share with...")
                        .child(state.clone())
                        .overlay_closable(true)
                        .close_button(false)
                        .on_ok({
                            let state_clone = state.clone();
                            let entity_clone = entity.clone();
                            move |_event: &ClickEvent, window, cx| {
                                let selected_ids = state_clone.read(cx).selected_ids.clone();
                                let result = entity_clone.update(cx, |this, cx| {
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
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .server_url
                                            .clone(),
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .email
                                            .clone(),
                                        this.settings
                                            .app_settings
                                            .synchronization_settings
                                            .key
                                            .clone(),
                                        payload,
                                    )
                                });
                                match result {
                                    Ok(_) => {
                                        log::info!("File shared successfully");
                                        let notification = (
                                            NotificationType::Success,
                                            SharedString::from("File shared successfully!"),
                                        );
                                        window.push_notification(notification, cx);
                                    }
                                    Err(e) => {
                                        log::error!("Failed to share file: {}", e);
                                        let notification = (
                                            NotificationType::Error,
                                            SharedString::from(format!(
                                                "Failed to share file: {}",
                                                e
                                            )),
                                        );
                                        window.push_notification(notification, cx);
                                    }
                                }
                                true
                            }
                        })
                });
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
