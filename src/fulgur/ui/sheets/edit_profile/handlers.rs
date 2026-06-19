use super::form_state::{
    DeviceKeyEdit, ProfileFormState, apply_device_key_edit, build_profile_from_form, optional_text,
    read_device_key_edit, rollback_device_key, snapshot_device_key_for_rollback,
};
use super::validation::{should_warn_for_http_url, validate_form, validate_url};
use crate::fulgur::Fulgur;
use crate::fulgur::sync::synchronization::perform_ping_with_progress;
use gpui::{App, Entity, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, notification::NotificationType,
    v_flex,
};
use std::sync::Arc;

/// Handle the Test connection button.
///
/// ### Arguments
/// - `entity`: The Fulgur entity (used to look up the existing public key).
/// - `state`: The shared form state.
/// - `window`: The window to attach the progress notification to.
/// - `cx`: The application context.
pub(super) fn handle_test_connection(
    entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
) {
    let url_raw = state.server_url_input.read(cx).value().trim().to_string();
    let Some(server_url) = optional_text(&url_raw) else {
        window.push_notification(
            (
                NotificationType::Error,
                SharedString::from("Enter a server URL to test the connection."),
            ),
            cx,
        );
        return;
    };
    if let Err(msg) = validate_url(&server_url) {
        window.push_notification((NotificationType::Error, msg), cx);
        return;
    }
    let device_key_edit = read_device_key_edit(state, cx);
    // Snapshot the existing key before any write so Cancel can restore it.
    if matches!(
        device_key_edit,
        DeviceKeyEdit::Set(_) | DeviceKeyEdit::Clear
    ) {
        snapshot_device_key_for_rollback(state);
    }
    if let Err(e) = apply_device_key_edit(&state.profile_id, &device_key_edit) {
        log::error!("Failed to save device API key for ping: {e}");
        window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!("Failed to save device key: {e}")),
            ),
            cx,
        );
        return;
    }
    let existing_public_key = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .find_profile(&state.profile_id)
        .and_then(|p| p.public_key.clone());
    let profile = build_profile_from_form(state, existing_public_key, cx);
    cx.global::<crate::fulgur::shared_state::SharedAppState>()
        .sync_state_for(&profile.id)
        .token_state
        .clear_token();
    let display_name = if profile.name.is_empty() {
        server_url
    } else {
        profile.name.clone()
    };
    perform_ping_with_progress(profile, display_name, window, cx);
}

/// Handle the Save button.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `state`: The shared form state.
/// - `window`: The window to attach notifications to.
/// - `cx`: The application context.
pub(super) fn handle_save(
    entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
) {
    handle_save_internal(entity, state, window, cx, true);
}

/// Handle profile persistence for the Save button, with optional HTTP warning.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `state`: The shared form state.
/// - `window`: The window to attach notifications/modals to.
/// - `cx`: The application context.
/// - `warn_on_http`: Whether to show an HTTP confirmation before saving.
fn handle_save_internal(
    entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
    warn_on_http: bool,
) {
    if let Err(message) = validate_form(state, entity, cx) {
        window.push_notification((NotificationType::Error, message), cx);
        return;
    }
    let server_url_value = state.server_url_input.read(cx).value().trim().to_string();
    if warn_on_http && should_warn_for_http_url(&server_url_value) {
        confirm_http_save(entity, state, window, cx);
        return;
    }
    let device_key_edit = read_device_key_edit(state, cx);
    if let Err(e) = apply_device_key_edit(&state.profile_id, &device_key_edit) {
        log::error!("Failed to save device API key: {e}");
        window.push_notification(
            (
                NotificationType::Error,
                SharedString::from(format!("Failed to save device key: {e}")),
            ),
            cx,
        );
        return;
    }
    let existing_public_key = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .find_profile(&state.profile_id)
        .and_then(|p| p.public_key.clone());
    let profile = build_profile_from_form(state, existing_public_key, cx);
    let profile_id = profile.id.clone();
    let is_new = state.is_new;
    let result = entity.update(cx, |this, cx| {
        if is_new {
            this.add_profile(profile, cx).map(|()| true)
        } else {
            this.update_profile(
                &profile_id,
                |existing| {
                    existing.name.clone_from(&profile.name);
                    existing.is_active = profile.is_active;
                    existing.server_url.clone_from(&profile.server_url);
                    existing.email.clone_from(&profile.email);
                    existing.is_deduplication = profile.is_deduplication;
                },
                cx,
            )
        }
    });
    match result {
        Ok(true) => {
            // Clear the cached JWT so the next call uses the latest key/email.
            cx.global::<crate::fulgur::shared_state::SharedAppState>()
                .sync_state_for(&profile_id)
                .token_state
                .clear_token();
            entity.update(cx, |this, cx| {
                this.restart_sse_connection_for_with_progress(&profile_id, window, cx);
            });
            window.close_sheet(cx);
        }
        Ok(false) => {
            log::warn!(
                "Profile '{profile_id}' could not be updated (no longer in settings); closing sheet"
            );
            window.close_sheet(cx);
        }
        Err(e) => {
            log::error!("Failed to save profile: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!("Failed to save profile: {e}")),
                ),
                cx,
            );
        }
    }
}

/// Ask for explicit confirmation before saving an `http://` server URL.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `state`: The shared form state.
/// - `window`: The window to attach the alert dialog to.
/// - `cx`: The application context.
fn confirm_http_save(
    entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
) {
    let server_url = state.server_url_input.read(cx).value().trim().to_string();
    let entity_for_confirm = entity.clone();
    let state_for_confirm = Arc::clone(state);
    window.open_alert_dialog(cx, move |modal, _, _| {
        let entity_ok = entity_for_confirm.clone();
        let state_ok = Arc::clone(&state_for_confirm);
        modal
            .title(div().text_size(px(16.)).child("Insecure HTTP connection"))
            .keyboard(true)
            .button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .cancel_text("Cancel")
                    .cancel_variant(ButtonVariant::Secondary)
                    .ok_text("Continue")
                    .ok_variant(ButtonVariant::Danger),
            )
            .overlay_closable(false)
            .close_button(false)
            .child(
                v_flex()
                    .gap_2()
                    .child(format!(
                        "The server URL \"{server_url}\" uses HTTP and can expose credentials in transit."
                    ))
                    .child("Are you sure you want to continue saving this server?"),
            )
            .on_ok(move |_, window, cx| {
                handle_save_internal(&entity_ok, &state_ok, window, cx, false);
                true
            })
            .on_cancel(|_, _, _| true)
    });
}

/// Handle the Cancel button.
///
/// ### Arguments
/// - `_entity`: The Fulgur entity (unused; reserved for future cleanup).
/// - `state`: The shared form state.
/// - `window`: The window the sheet is attached to.
/// - `cx`: The application context.
pub(super) fn handle_cancel(
    _entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
) {
    if let Err(e) = rollback_device_key(state) {
        log::warn!(
            "Failed to roll back device key for profile '{}': {e}",
            state.profile_id
        );
    }
    window.close_sheet(cx);
}

/// Open a confirmation dialog before deleting a profile.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `state`: The shared form state (read for profile id and name).
/// - `window`: The window to attach the dialog to.
/// - `cx`: The application context.
pub(super) fn confirm_delete_profile(
    entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
) {
    let profile_name = state.name_input.read(cx).value().trim().to_string();
    let display_name = if profile_name.is_empty() {
        "this server".to_string()
    } else {
        format!("'{profile_name}'")
    };
    let entity_for_confirm = entity.clone();
    let state_for_confirm = Arc::clone(state);
    window.open_alert_dialog(cx, move |modal, _, _| {
        let entity_ok = entity_for_confirm.clone();
        let state_ok = Arc::clone(&state_for_confirm);
        modal
            .title(div().text_size(px(16.)).child("Delete Server"))
            .keyboard(true)
            .button_props(
                DialogButtonProps::default()
                    .show_cancel(true)
                    .cancel_text("Cancel")
                    .cancel_variant(ButtonVariant::Secondary)
                    .ok_text("Delete")
                    .ok_variant(ButtonVariant::Danger),
            )
            .overlay_closable(false)
            .close_button(false)
            .child(
                v_flex()
                    .gap_2()
                    .child(format!("Delete {display_name}?"))
                    .child("This cannot be undone."),
            )
            .on_ok(move |_, window, cx| {
                let profile_id = state_ok.profile_id.clone();
                let outcome = entity_ok.update(cx, |this, cx| this.delete_profile(&profile_id, cx));
                match outcome {
                    Ok(_) => {
                        window.close_sheet(cx);
                    }
                    Err(e) => {
                        log::error!("Failed to delete profile '{profile_id}': {e}");
                        window.push_notification(
                            (
                                NotificationType::Error,
                                SharedString::from(format!("Failed to delete server: {e}")),
                            ),
                            cx,
                        );
                    }
                }
                true
            })
            .on_cancel(|_, _, _| true)
    });
}
