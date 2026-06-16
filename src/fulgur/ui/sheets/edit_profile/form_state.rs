use crate::fulgur::settings::{ProfileId, ServerProfile};
use crate::fulgur::utils::crypto_helper::save_device_api_key_to_keychain;
use gpui::{App, Entity};
use gpui_component::input::InputState;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

pub(super) const DEVICE_KEY_PLACEHOLDER: &str = "<Device Key>";

/// Form state shared between the sheet body, the Test connection button, and the Save handler.
pub(super) struct ProfileFormState {
    pub(super) profile_id: ProfileId, //Identifier of the profile being edited (or freshly minted for Add mode)
    pub(super) is_new: bool,          //True when this sheet is creating a new profile
    pub(super) name_input: Entity<InputState>,
    pub(super) server_url_input: Entity<InputState>,
    pub(super) email_input: Entity<InputState>,
    pub(super) device_key_input: Entity<InputState>,
    pub(super) is_active: Arc<Mutex<bool>>, //Per-profile activation flag held in a shared mutex
    pub(super) is_deduplication: Arc<Mutex<bool>>,
    pub(super) device_key_written_for_add: Arc<AtomicBool>, //Used by the Cancel path to roll back that write so the keychain stays clea
}

/// Build a profile draft from the current form values.
///
/// ### Arguments
/// - `state`: The shared form state.
/// - `existing_public_key`: Existing public key to carry over for Edit mode;
///   `None` for Add mode (key generation happens later if needed).
/// - `cx`: The application context (used to read input values).
///
/// ### Returns
/// - `ServerProfile`: A profile struct populated from the form.
pub(super) fn build_profile_from_form(
    state: &ProfileFormState,
    existing_public_key: Option<String>,
    cx: &App,
) -> ServerProfile {
    let name = state.name_input.read(cx).value().trim().to_string();
    let server_url = optional_text(&state.server_url_input.read(cx).value());
    let email = optional_text(&state.email_input.read(cx).value());
    ServerProfile {
        id: state.profile_id.clone(),
        name,
        is_active: *state.is_active.lock(),
        server_url,
        email,
        public_key: existing_public_key,
        is_deduplication: *state.is_deduplication.lock(),
    }
}

/// Convert an input value into the optional string used by `ServerProfile`.
///
/// ### Arguments
/// - `value`: The trimmed/untrimmed input string.
///
/// ### Returns
/// - `Some(String)`: The trimmed value when non-empty.
/// - `None`: When the trimmed value is empty.
pub(super) fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

pub(super) enum DeviceKeyEdit {
    Untouched,
    Clear,
    Set(String),
}

/// Read the current device key value from the form.
///
/// ### Arguments
/// - `state`: The shared form state.
/// - `cx`: The application context.
///
/// ### Returns
/// - `DeviceKeyEdit::Untouched`: The user has not modified the placeholder.
/// - `DeviceKeyEdit::Clear`: The user erased the field.
/// - `DeviceKeyEdit::Set(value)`: The user typed a new key.
pub(super) fn read_device_key_edit(state: &ProfileFormState, cx: &App) -> DeviceKeyEdit {
    let raw = state.device_key_input.read(cx).value().to_string();
    if raw == DEVICE_KEY_PLACEHOLDER {
        DeviceKeyEdit::Untouched
    } else if raw.is_empty() {
        DeviceKeyEdit::Clear
    } else {
        DeviceKeyEdit::Set(raw)
    }
}

/// Apply a `DeviceKeyEdit` to the keychain for a profile.
///
/// ### Arguments
/// - `profile_id`: The id whose keychain entries should be updated.
/// - `edit`: The user's edit, as decoded by `read_device_key_edit`.
///
/// ### Returns
/// - `Ok(true)`: A keychain write happened (set or clear).
/// - `Ok(false)`: The user did not modify the field; nothing was written.
/// - `Err(anyhow::Error)`: The keychain operation failed.
pub(super) fn apply_device_key_edit(
    profile_id: &str,
    edit: &DeviceKeyEdit,
) -> anyhow::Result<bool> {
    match edit {
        DeviceKeyEdit::Untouched => Ok(false),
        DeviceKeyEdit::Clear => {
            save_device_api_key_to_keychain(profile_id, None)?;
            Ok(true)
        }
        DeviceKeyEdit::Set(value) => {
            save_device_api_key_to_keychain(profile_id, Some(value))?;
            Ok(true)
        }
    }
}
