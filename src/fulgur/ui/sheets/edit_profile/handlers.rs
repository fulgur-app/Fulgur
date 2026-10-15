use super::form_state::{
    DeviceKeyEdit, ProfileFormState, apply_device_key_edit, build_profile_from_form,
    discard_device_key_rollback, optional_text, read_device_key_edit, rollback_device_key,
    snapshot_device_key_for_rollback,
};
use super::validation::{should_warn_for_http_url, validate_form, validate_url};
use crate::fulgur::Fulgur;
use crate::fulgur::sync::synchronization::perform_ping_with_progress;
use gpui_kit::component::{
    WindowExt, button::ButtonVariant, dialog::DialogButtonProps, notification::NotificationType,
    v_flex,
};
use gpui_kit::{App, Entity, ParentElement, SharedString, Styled, Window, div, px};
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
    let is_active = profile.is_active;
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
            discard_device_key_rollback(state);
            // Clear the cached JWT so the next call uses the latest key/email.
            cx.global::<crate::fulgur::shared_state::SharedAppState>()
                .sync_state_for(&profile_id)
                .token_state
                .clear_token();
            entity.update(cx, |this, cx| {
                if is_active {
                    this.restart_sse_connection_for_with_progress(&profile_id, window, cx);
                } else {
                    this.stop_sse_connection_for(&profile_id, cx);
                }
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
    handle_dismiss(state);
    window.close_sheet(cx);
}

/// Undo any Test-connection keychain write when the sheet is dismissed without saving.
///
/// ### Arguments
/// - `state`: The shared form state.
pub(super) fn handle_dismiss(state: &ProfileFormState) {
    if let Err(e) = rollback_device_key(state) {
        log::warn!(
            "Failed to roll back device key for profile '{}': {e}",
            state.profile_id
        );
    }
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
                        discard_device_key_rollback(&state_ok);
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

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{confirm_delete_profile, handle_cancel, handle_dismiss, handle_save};
    use crate::fulgur::Fulgur;
    use crate::fulgur::settings::ServerProfile;
    use crate::fulgur::ui::sheets::edit_profile::form_state::{
        ProfileFormState, apply_device_key_edit, gpui_tests::make_form_state, read_device_key_edit,
        snapshot_device_key_for_rollback,
    };
    use crate::fulgur::utils::crypto_helper::{
        load_device_api_key_from_keychain, save_device_api_key_to_keychain,
    };
    use crate::test_support::{
        dispatch_dialog_action, has_active_dialog, open_rendered_window, setup_test_globals_with,
    };
    use gpui_kit::base::actions::{Cancel, Confirm};
    use gpui_kit::{Entity, TestAppContext, VisualTestContext};
    use std::sync::Arc;

    const TYPED_KEY: &str = "typed-key";
    const ORIGINAL_KEY: &str = "original-key";

    /// Seed settings with an optional profile, then open a rendered `Fulgur` window.
    ///
    /// ### Arguments
    /// - `existing`: The profile to register before the window opens, if any.
    /// - `cx`: The test application context.
    ///
    /// ### Returns
    /// - `(Entity<Fulgur>, VisualTestContext)`: The Fulgur entity and its visual test context.
    fn open_window_with_profile(
        existing: Option<ServerProfile>,
        cx: &mut TestAppContext,
    ) -> (Entity<Fulgur>, VisualTestContext) {
        setup_test_globals_with(cx, |settings| {
            settings
                .app_settings
                .synchronization_settings
                .profiles
                .extend(existing);
        });
        let (fulgur, _handle, visual_cx) = open_rendered_window(cx);
        (fulgur, visual_cx)
    }

    /// Build an inactive profile with a fixed id, so a test owns its keychain entry.
    ///
    /// ### Arguments
    /// - `profile_id`: The id to assign.
    /// - `name`: The profile name.
    ///
    /// ### Returns
    /// - `ServerProfile`: The profile, with activation off so saving never opens a connection.
    fn inactive_profile(profile_id: &str, name: &str) -> ServerProfile {
        let mut profile = ServerProfile::new(name);
        profile.id = profile_id.to_string();
        profile
    }

    /// Build the shared form state the sheet hands to its handlers, with `TYPED_KEY` typed in.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window hosting the form.
    /// - `profile_id`: The id of the profile being edited or created.
    /// - `is_new`: Whether the form creates a new profile.
    /// - `name`: The server name typed in the form.
    /// - `url`: The server URL typed in the form.
    ///
    /// ### Returns
    /// - `Arc<ProfileFormState>`: The form state for an inactive profile.
    fn open_form(
        visual_cx: &mut VisualTestContext,
        profile_id: &str,
        is_new: bool,
        name: &str,
        url: &str,
    ) -> Arc<ProfileFormState> {
        visual_cx.update(|window, cx| {
            Arc::new(make_form_state(
                profile_id, is_new, name, url, "", TYPED_KEY, false, false, window, cx,
            ))
        })
    }

    /// Write the typed device key the way the Test connection button does, snapshot first.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context of the window hosting the form.
    /// - `state`: The form state whose device key is written.
    fn write_key_as_test_connection(visual_cx: &mut VisualTestContext, state: &ProfileFormState) {
        visual_cx.update(|_window, cx| {
            snapshot_device_key_for_rollback(state);
            apply_device_key_edit(&state.profile_id, &read_device_key_edit(state, cx))
                .expect("test-connection write");
        });
    }

    /// Read the device key currently stored for a profile.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose key is read.
    ///
    /// ### Returns
    /// - `Some(String)`: The stored key.
    /// - `None`: No key is stored.
    fn stored_key(profile_id: &str) -> Option<String> {
        load_device_api_key_from_keychain(profile_id).expect("read keychain")
    }

    /// Look up a profile in the window's settings.
    ///
    /// ### Arguments
    /// - `fulgur`: The Fulgur entity owning the settings.
    /// - `visual_cx`: The visual test context of its window.
    /// - `profile_id`: The id to look up.
    ///
    /// ### Returns
    /// - `Some(ServerProfile)`: The registered profile.
    /// - `None`: No profile with that id is registered.
    fn find_profile(
        fulgur: &Entity<Fulgur>,
        visual_cx: &mut VisualTestContext,
        profile_id: &str,
    ) -> Option<ServerProfile> {
        visual_cx.update(|_window, cx| {
            fulgur
                .read(cx)
                .settings
                .app_settings
                .synchronization_settings
                .find_profile(profile_id)
                .cloned()
        })
    }

    #[gpui_kit::test]
    fn cancel_restores_the_key_replaced_by_test_connection(cx: &mut TestAppContext) {
        let id = "handlers-cancel-existing";
        let (fulgur, mut visual_cx) =
            open_window_with_profile(Some(inactive_profile(id, "Server")), cx);
        save_device_api_key_to_keychain(id, Some(ORIGINAL_KEY)).expect("seed keychain");
        let state = open_form(&mut visual_cx, id, false, "Server", "");
        write_key_as_test_connection(&mut visual_cx, &state);
        assert_eq!(stored_key(id).as_deref(), Some(TYPED_KEY));

        visual_cx.update(|window, cx| handle_cancel(&fulgur, &state, window, cx));

        assert_eq!(stored_key(id).as_deref(), Some(ORIGINAL_KEY));
        save_device_api_key_to_keychain(id, None).expect("clean keychain");
    }

    #[gpui_kit::test]
    fn cancel_on_a_new_profile_leaves_no_key_behind(cx: &mut TestAppContext) {
        let id = "handlers-cancel-new";
        let (fulgur, mut visual_cx) = open_window_with_profile(None, cx);
        let state = open_form(&mut visual_cx, id, true, "Fresh", "");
        write_key_as_test_connection(&mut visual_cx, &state);

        visual_cx.update(|window, cx| handle_cancel(&fulgur, &state, window, cx));

        assert_eq!(stored_key(id), None);
        assert!(find_profile(&fulgur, &mut visual_cx, id).is_none());
    }

    #[gpui_kit::test]
    fn save_commits_the_profile_and_a_later_dismissal_keeps_the_key(cx: &mut TestAppContext) {
        let id = "handlers-save-new";
        let (fulgur, mut visual_cx) = open_window_with_profile(None, cx);
        let state = open_form(&mut visual_cx, id, true, "Saved", "https://example.com");
        write_key_as_test_connection(&mut visual_cx, &state);

        visual_cx.update(|window, cx| handle_save(&fulgur, &state, window, cx));
        handle_dismiss(&state);

        let profile = find_profile(&fulgur, &mut visual_cx, id).expect("profile saved");
        assert_eq!(profile.name, "Saved");
        assert_eq!(profile.server_url.as_deref(), Some("https://example.com"));
        assert_eq!(stored_key(id).as_deref(), Some(TYPED_KEY));
        save_device_api_key_to_keychain(id, None).expect("clean keychain");
    }

    #[gpui_kit::test]
    fn save_updates_an_existing_profile_in_place(cx: &mut TestAppContext) {
        let id = "handlers-save-existing";
        let (fulgur, mut visual_cx) =
            open_window_with_profile(Some(inactive_profile(id, "Before")), cx);
        let state = open_form(&mut visual_cx, id, false, "After", "");

        visual_cx.update(|window, cx| handle_save(&fulgur, &state, window, cx));

        let profile = find_profile(&fulgur, &mut visual_cx, id).expect("profile kept");
        assert_eq!(profile.name, "After");
        assert_eq!(stored_key(id).as_deref(), Some(TYPED_KEY));
        save_device_api_key_to_keychain(id, None).expect("clean keychain");
    }

    #[gpui_kit::test]
    fn invalid_save_writes_neither_settings_nor_keychain(cx: &mut TestAppContext) {
        let id = "handlers-save-invalid";
        let (fulgur, mut visual_cx) =
            open_window_with_profile(Some(inactive_profile(id, "Server")), cx);
        save_device_api_key_to_keychain(id, Some(ORIGINAL_KEY)).expect("seed keychain");
        let state = open_form(&mut visual_cx, id, false, "   ", "");

        visual_cx.update(|window, cx| handle_save(&fulgur, &state, window, cx));

        let profile = find_profile(&fulgur, &mut visual_cx, id).expect("profile kept");
        assert_eq!(profile.name, "Server");
        assert_eq!(stored_key(id).as_deref(), Some(ORIGINAL_KEY));
        save_device_api_key_to_keychain(id, None).expect("clean keychain");
    }

    #[gpui_kit::test]
    fn http_save_waits_for_confirmation(cx: &mut TestAppContext) {
        let id = "handlers-save-http";
        let (fulgur, mut visual_cx) = open_window_with_profile(None, cx);
        let state = open_form(&mut visual_cx, id, true, "Plain", "http://example.com");

        visual_cx.update(|window, cx| handle_save(&fulgur, &state, window, cx));

        assert!(has_active_dialog(&mut visual_cx));
        assert!(find_profile(&fulgur, &mut visual_cx, id).is_none());
        assert_eq!(stored_key(id), None);

        dispatch_dialog_action(&mut visual_cx, Confirm { secondary: false });

        assert!(find_profile(&fulgur, &mut visual_cx, id).is_some());
        assert_eq!(stored_key(id).as_deref(), Some(TYPED_KEY));
        save_device_api_key_to_keychain(id, None).expect("clean keychain");
    }

    #[gpui_kit::test]
    fn cancelled_delete_keeps_the_profile(cx: &mut TestAppContext) {
        let id = "handlers-delete-cancel";
        let (fulgur, mut visual_cx) =
            open_window_with_profile(Some(inactive_profile(id, "Server")), cx);
        let state = open_form(&mut visual_cx, id, false, "Server", "");

        visual_cx.update(|window, cx| confirm_delete_profile(&fulgur, &state, window, cx));
        dispatch_dialog_action(&mut visual_cx, Cancel);

        assert!(!has_active_dialog(&mut visual_cx));
        assert!(find_profile(&fulgur, &mut visual_cx, id).is_some());
    }

    #[gpui_kit::test]
    fn confirmed_delete_removes_the_profile_without_restoring_its_key(cx: &mut TestAppContext) {
        let id = "handlers-delete-confirm";
        let (fulgur, mut visual_cx) =
            open_window_with_profile(Some(inactive_profile(id, "Server")), cx);
        save_device_api_key_to_keychain(id, Some(ORIGINAL_KEY)).expect("seed keychain");
        let state = open_form(&mut visual_cx, id, false, "Server", "");
        write_key_as_test_connection(&mut visual_cx, &state);

        visual_cx.update(|window, cx| confirm_delete_profile(&fulgur, &state, window, cx));
        dispatch_dialog_action(&mut visual_cx, Confirm { secondary: false });
        handle_dismiss(&state);

        assert!(find_profile(&fulgur, &mut visual_cx, id).is_none());
        assert_eq!(stored_key(id), None);
    }
}
