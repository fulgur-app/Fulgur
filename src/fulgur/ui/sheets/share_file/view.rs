use super::actions::handle_share_file;
use super::state::{ProfileFetchState, ShareSheetState};
use crate::fulgur::{
    Fulgur,
    settings::{ProfileId, ServerProfile},
    sync::share::{Device, get_icon},
    ui::icons::CustomIcon,
};
use gpui::{
    App, Div, Element, Entity, FontWeight, InteractiveElement, ParentElement,
    StatefulInteractiveElement, Styled, div, prelude::FluentBuilder,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    spinner::Spinner,
    v_flex,
};
use parking_lot::Mutex;
use std::sync::Arc;

/// Create a single device row in the share sheet.
///
/// ### Arguments
/// - `profile_id`: The id of the profile that owns this device.
/// - `device`: The device to display.
/// - `is_selected`: Whether the device is currently selected.
/// - `selected_keys`: Shared mutable selection state, keyed by `(profile_id, device_id)`.
/// - `idx`: Stable index used to disambiguate the GPUI element id.
/// - `cx`: The application context.
///
/// ### Returns
/// - `impl Element`: The device row element.
fn make_device_item(
    profile_id: &ProfileId,
    device: &Device,
    is_selected: bool,
    selected_keys: Arc<Mutex<Vec<(ProfileId, String)>>>,
    idx: usize,
    cx: &App,
) -> impl Element {
    let profile_id = profile_id.clone();
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
        .when(has_public_key, gpui::Styled::cursor_pointer)
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
                        .child(div().text_xs().child(format!("Expires: {device_expires}"))),
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
                let mut keys = selected_keys.lock();
                let key = (profile_id.clone(), device_id.clone());
                if let Some(pos) = keys.iter().position(|existing| existing == &key) {
                    keys.remove(pos);
                } else {
                    keys.push(key);
                }
            })
        })
}

/// Build a profile section header used to delimit per-profile device rows.
///
/// ### Arguments
/// - `profile`: The profile to label.
/// - `cx`: The application context (used to read theme tokens).
///
/// ### Returns
/// - `Div`: The styled header element.
fn make_profile_header(profile: &ServerProfile, cx: &App) -> Div {
    let url_label = profile
        .server_url
        .clone()
        .unwrap_or_else(|| "(no URL)".to_string());
    v_flex()
        .gap_0p5()
        .pt_2()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(profile.name.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(url_label),
        )
}

/// Render the placeholder shown while a profile's device list is loading.
///
/// ### Arguments
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The loading placeholder element.
fn make_loading_section(cx: &App) -> Div {
    h_flex()
        .gap_2()
        .items_center()
        .my_2()
        .child(Spinner::new().icon(CustomIcon::LoaderCircle).small())
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Loading devices..."),
        )
}

/// Render the failure message for a profile whose device fetch errored.
///
/// ### Arguments
/// - `message`: The error description from the worker thread.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The styled error placeholder.
fn make_error_section(message: &str, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().danger)
        .my_2()
        .child(format!("Could not reach this profile: {message}"))
}

/// Render the empty-state placeholder when a profile returned zero devices.
///
/// ### Arguments
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The empty-state element.
fn make_empty_section(cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .my_2()
        .child("No devices available.")
}

/// Build the grouped device list reflecting the current per-profile state.
///
/// Each profile (header + its devices, loading placeholder, or error message)
/// is wrapped in its own `v_flex` so the outer container can apply a wider
/// gap between profile groups than between rows inside a group.
///
/// ### Arguments
/// - `state`: Shared sheet state.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The grouped list element.
pub(super) fn make_device_list(state: &Arc<ShareSheetState>, cx: &App) -> Div {
    let mut container = v_flex().gap_6();
    let mut row_idx: usize = 0;
    let map = state.per_profile.read();
    for profile in &state.profiles {
        let mut group = div().child(make_profile_header(profile, cx));
        match map.get(&profile.id) {
            None | Some(ProfileFetchState::Loading) => {
                group = group.child(make_loading_section(cx));
            }
            Some(ProfileFetchState::Failed(message)) => {
                group = group.child(make_error_section(message, cx));
            }
            Some(ProfileFetchState::Loaded(devices)) => {
                if devices.is_empty() {
                    group = group.child(make_empty_section(cx));
                } else {
                    for device in devices.iter() {
                        let is_selected = state
                            .selected
                            .lock()
                            .iter()
                            .any(|(pid, did)| pid == &profile.id && did == &device.id);
                        group = group.child(make_device_item(
                            &profile.id,
                            device,
                            is_selected,
                            state.selected.clone(),
                            row_idx,
                            cx,
                        ));
                        row_idx += 1;
                    }
                }
            }
        }
        container = container.child(group);
    }
    container
}

/// Render the share sheet footer with Cancel and Share buttons.
///
/// ### Arguments
/// - `state`: Shared sheet state.
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `Div`: The footer element.
pub(super) fn render_footer(state: Arc<ShareSheetState>, entity: Entity<Fulgur>) -> Div {
    let state_for_cancel = Arc::clone(&state);
    let entity_for_cancel = entity.clone();
    let state_for_share = state;
    let entity_for_share = entity;
    h_flex()
        .justify_end()
        .w_full()
        .gap_2()
        .child(
            Button::new("cancel-share")
                .child("Cancel")
                .small()
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    state_for_cancel
                        .active
                        .store(false, std::sync::atomic::Ordering::Release);
                    entity_for_cancel.update(cx, |this, _| {
                        this.share_sheet_state = None;
                    });
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
                    handle_share_file(&state_for_share, &entity_for_share, window, cx);
                }),
        )
}
