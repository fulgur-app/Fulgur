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
