use crate::fulgur::{
    Fulgur,
    settings::{MAX_PROFILES, ProfileId, ServerProfile, new_profile_id},
    sync::synchronization::perform_ping_with_progress,
    utils::crypto_helper::save_device_api_key_to_keychain,
};
use gpui::{
    App, AppContext, Context, Entity, FontWeight, IntoElement, ParentElement, SharedString, Styled,
    Window, div, prelude::FluentBuilder, px,
};
use gpui_component::{
    ActiveTheme, Sizable, WindowExt,
    button::{Button, ButtonVariant, ButtonVariants},
    dialog::DialogButtonProps,
    h_flex,
    input::{Input, InputState},
    notification::NotificationType,
    switch::Switch,
    v_flex,
};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

const DEVICE_KEY_PLACEHOLDER: &str = "<Device Key>";

/// Form state shared between the sheet body, the Test connection button, and the Save handler.
struct ProfileFormState {
    profile_id: ProfileId, //Identifier of the profile being edited (or freshly minted for Add mode)
    is_new: bool,          //True when this sheet is creating a new profile
    name_input: Entity<InputState>,
    server_url_input: Entity<InputState>,
    email_input: Entity<InputState>,
    device_key_input: Entity<InputState>,
    is_active: Arc<Mutex<bool>>, //Per-profile activation flag held in a shared mutex
    is_deduplication: Arc<Mutex<bool>>,
    device_key_written_for_add: Arc<AtomicBool>, //Used by the Cancel path to roll back that write so the keychain stays clea
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
fn build_profile_from_form(
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
fn optional_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Validate a server URL string from the form.
///
/// ### Arguments
/// - `value`: The form value (already trimmed by the caller).
///
/// ### Returns
/// - `Ok(())`: The URL is empty (allowed) or parses successfully.
/// - `Err(SharedString)`: A user-facing error message describing the failure.
fn validate_url(value: &str) -> Result<(), SharedString> {
    if value.is_empty() {
        return Ok(());
    }
    url::Url::parse(value)
        .map(|_| ())
        .map_err(|_| SharedString::from("Server URL is not a valid URL."))
}

/// Validate an email string from the form using the same heuristic the
/// settings validator uses (presence of `@` and a `.` after it).
///
/// ### Arguments
/// - `value`: The form value (already trimmed by the caller).
///
/// ### Returns
/// - `Ok(())`: The email is empty (allowed) or passes the heuristic.
/// - `Err(SharedString)`: A user-facing error message describing the failure.
fn validate_email(value: &str) -> Result<(), SharedString> {
    if value.is_empty() {
        return Ok(());
    }
    let at_pos = value.find('@');
    let is_valid = at_pos
        .is_some_and(|pos| pos > 0 && pos < value.len() - 1 && value[pos + 1..].contains('.'));
    if is_valid {
        Ok(())
    } else {
        Err(SharedString::from("Email address is not valid."))
    }
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
enum DeviceKeyEdit {
    Untouched,
    Clear,
    Set(String),
}

fn read_device_key_edit(state: &ProfileFormState, cx: &App) -> DeviceKeyEdit {
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
fn apply_device_key_edit(profile_id: &str, edit: &DeviceKeyEdit) -> anyhow::Result<bool> {
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

impl Fulgur {
    /// Open the Add/Edit Profile sheet.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile to edit, or `None` to add a new one.
    /// - `window`: The window to attach the sheet to.
    /// - `cx`: The Fulgur context.
    pub fn open_edit_profile_sheet(
        &mut self,
        profile_id: Option<&str>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if profile_id.is_none()
            && self
                .settings
                .app_settings
                .synchronization_settings
                .profiles
                .len()
                >= MAX_PROFILES
        {
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from(format!(
                        "Maximum of {MAX_PROFILES} Fulgurant instances reached."
                    )),
                ),
                cx,
            );
            return;
        }

        let (
            profile_id,
            is_new,
            initial_name,
            initial_active,
            initial_url,
            initial_email,
            initial_dedup,
        ) = match profile_id.and_then(|id| {
            self.settings
                .app_settings
                .synchronization_settings
                .find_profile(id)
                .cloned()
        }) {
            Some(profile) => (
                profile.id,
                false,
                profile.name,
                profile.is_active,
                profile.server_url.unwrap_or_default(),
                profile.email.unwrap_or_default(),
                profile.is_deduplication,
            ),
            None => (
                new_profile_id(),
                true,
                String::new(),
                true,
                String::new(),
                String::new(),
                true,
            ),
        };

        let name_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("Server name")
                .default_value(initial_name)
        });
        let server_url_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("https://example.com")
                .default_value(initial_url)
        });
        let email_input = cx.new(|cx| {
            InputState::new(window, cx)
                .placeholder("you@example.com")
                .default_value(initial_email)
        });
        let device_key_input = cx.new(|cx| {
            if is_new {
                InputState::new(window, cx).placeholder(DEVICE_KEY_PLACEHOLDER)
            } else {
                InputState::new(window, cx)
                    .default_value(SharedString::from(DEVICE_KEY_PLACEHOLDER))
            }
        });

        let state = Arc::new(ProfileFormState {
            profile_id,
            is_new,
            name_input,
            server_url_input,
            email_input,
            device_key_input,
            is_active: Arc::new(Mutex::new(initial_active)),
            is_deduplication: Arc::new(Mutex::new(initial_dedup)),
            device_key_written_for_add: Arc::new(AtomicBool::new(false)),
        });

        let entity = cx.entity();
        let viewport_height = window.viewport_size().height;
        let title: SharedString = if is_new {
            "Add Fulgurant instance"
        } else {
            "Edit Fulgurant instance"
        }
        .into();

        window.open_sheet(cx, move |sheet, _window, cx| {
            let state_for_body = Arc::clone(&state);
            let state_for_save = Arc::clone(&state);
            let state_for_begin = Arc::clone(&state);
            let state_for_delete = Arc::clone(&state);
            let state_for_cancel = Arc::clone(&state);
            let entity_save = entity.clone();
            let entity_begin = entity.clone();
            let entity_delete = entity.clone();
            let entity_cancel = entity.clone();
            #[cfg(target_os = "linux")]
            let sheet_overhead = px(220.0);
            #[cfg(not(target_os = "linux"))]
            let sheet_overhead = px(170.0);
            let max_height = px((viewport_height - sheet_overhead).into());
            sheet
                .title(title.clone())
                .size(px(440.))
                .overlay(true)
                .child(
                    v_flex()
                        .gap_3()
                        .h(max_height)
                        .child(render_form_body(&state_for_body, cx)),
                )
                .footer({
                    let mut footer = h_flex().w_full().gap_2().justify_between();
                    if is_new {
                        footer = footer.child(div());
                    } else {
                        let state_for_delete_inner = Arc::clone(&state_for_delete);
                        let entity_delete_inner = entity_delete.clone();
                        footer = footer.child(
                            Button::new("delete-profile")
                                .child("Delete")
                                .small()
                                .danger()
                                .cursor_pointer()
                                .on_click(move |_, window, cx| {
                                    confirm_delete_profile(
                                        &entity_delete_inner,
                                        &state_for_delete_inner,
                                        window,
                                        cx,
                                    );
                                }),
                        );
                    }
                    let state_begin_inner = Arc::clone(&state_for_begin);
                    let entity_begin_inner = entity_begin.clone();
                    let state_save_inner = Arc::clone(&state_for_save);
                    let entity_save_inner = entity_save.clone();
                    let state_cancel_inner = Arc::clone(&state_for_cancel);
                    let entity_cancel_inner = entity_cancel.clone();
                    footer.child(
                        h_flex()
                            .gap_2()
                            .child(
                                Button::new("test-connection-from-sheet")
                                    .child("Test connection")
                                    .small()
                                    .cursor_pointer()
                                    .on_click(move |_, window, cx| {
                                        handle_test_connection(
                                            &entity_begin_inner,
                                            &state_begin_inner,
                                            window,
                                            cx,
                                        );
                                    }),
                            )
                            .child(
                                Button::new("cancel-edit-profile")
                                    .child("Cancel")
                                    .small()
                                    .cursor_pointer()
                                    .on_click(move |_, window, cx| {
                                        handle_cancel(
                                            &entity_cancel_inner,
                                            &state_cancel_inner,
                                            window,
                                            cx,
                                        );
                                    }),
                            )
                            .child(
                                Button::new("save-edit-profile")
                                    .child("Save")
                                    .small()
                                    .primary()
                                    .cursor_pointer()
                                    .on_click(move |_, window, cx| {
                                        handle_save(
                                            &entity_save_inner,
                                            &state_save_inner,
                                            window,
                                            cx,
                                        );
                                    }),
                            ),
                    )
                })
        });
    }
}

/// Render the body of the edit profile sheet.
///
/// ### Arguments
/// - `state`: The shared form state.
/// - `cx`: The application context (used to read theme tokens).
///
/// ### Returns
/// - `impl IntoElement`: The form body, ready to be attached to the sheet.
fn render_form_body(state: &Arc<ProfileFormState>, cx: &App) -> impl IntoElement {
    let active_for_switch = Arc::clone(&state.is_active);
    let dedup_for_switch = Arc::clone(&state.is_deduplication);
    let activate_checked = *state.is_active.lock();
    let dedup_checked = *state.is_deduplication.lock();

    v_flex()
        .gap_4()
        .child(field_label("Name", cx))
        .child(Input::new(&state.name_input))
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .gap_3()
                .child(field_label("Activate this server", cx))
                .child(
                    Switch::new("edit-profile-activate")
                        .checked(activate_checked)
                        .on_click(move |val: &bool, _window, _cx| {
                            *active_for_switch.lock() = *val;
                        }),
                ),
        )
        .child(field_label("Server URL", cx))
        .child(Input::new(&state.server_url_input))
        .child(field_label("Email", cx))
        .child(Input::new(&state.email_input))
        .child(field_label("Device Key", cx))
        .child(Input::new(&state.device_key_input))
        .when(!state.is_new, |el| {
            el.child(
                div()
                    .text_xs()
                    .text_color(cx.theme().muted_foreground)
                    .child(
                        "Device key is stored in the system keychain. Leave the placeholder to keep the existing key.",
                    ),
            )
        })
        .child(
            h_flex()
                .w_full()
                .justify_between()
                .items_center()
                .gap_3()
                .child(field_label("Deduplication", cx))
                .child(
                    Switch::new("edit-profile-dedup")
                        .checked(dedup_checked)
                        .on_click(move |val: &bool, _window, _cx| {
                            *dedup_for_switch.lock() = *val;
                        }),
                ),
        )
}

/// Render a form field label with a consistent style.
///
/// ### Arguments
/// - `label`: The label text.
/// - `cx`: The application context (used to read theme tokens).
///
/// ### Returns
/// - `impl IntoElement`: The styled label element.
fn field_label(label: &str, cx: &App) -> impl IntoElement {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().foreground)
        .child(label.to_string())
}

/// Handle the Test connection button.
///
/// ### Description
/// Saves the device key from the form to the keychain (if changed), then
/// pings the Fulgurant server with a fresh JWT token. The result is shown
/// as a success or error notification. No profile changes are persisted.
///
/// ### Arguments
/// - `entity`: The Fulgur entity (used to look up the existing public key).
/// - `state`: The shared form state.
/// - `window`: The window to attach the progress notification to.
/// - `cx`: The application context.
fn handle_test_connection(
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
    if state.is_new && matches!(device_key_edit, DeviceKeyEdit::Set(_)) {
        state
            .device_key_written_for_add
            .store(true, Ordering::Relaxed);
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
fn handle_save(
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

/// Check whether the save flow should warn the user about insecure transport.
///
/// ### Arguments
/// - `server_url`: The trimmed server URL string from the form.
///
/// ### Returns
/// - `true`: The URL parses and uses the `http` scheme.
/// - `false`: The URL is empty, invalid, or uses another scheme.
fn should_warn_for_http_url(server_url: &str) -> bool {
    if server_url.is_empty() {
        return false;
    }
    url::Url::parse(server_url).is_ok_and(|url| url.scheme().eq_ignore_ascii_case("http"))
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
fn handle_cancel(
    _entity: &Entity<Fulgur>,
    state: &Arc<ProfileFormState>,
    window: &mut Window,
    cx: &mut App,
) {
    if state.is_new
        && state.device_key_written_for_add.load(Ordering::Relaxed)
        && let Err(e) = save_device_api_key_to_keychain(&state.profile_id, None)
    {
        log::warn!(
            "Failed to clean up draft device key for profile '{}': {e}",
            state.profile_id
        );
    }
    window.close_sheet(cx);
}

/// Validate every field in the form, returning the first failure as a
/// user-facing message.
///
/// ### Arguments
/// - `state`: The shared form state.
/// - `entity`: The Fulgur entity (used to check name uniqueness against the
///   current profile list).
/// - `cx`: The application context.
///
/// ### Returns
/// - `Ok(())`: All fields pass validation.
/// - `Err(SharedString)`: A user-facing error message.
fn validate_form(
    state: &ProfileFormState,
    entity: &Entity<Fulgur>,
    cx: &App,
) -> Result<(), SharedString> {
    let name = state.name_input.read(cx).value().trim().to_string();
    if name.is_empty() {
        return Err(SharedString::from("Server name cannot be empty."));
    }
    let collides = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .name_collides(&name, Some(&state.profile_id));
    if collides {
        return Err(SharedString::from("Another server already uses this name."));
    }
    let url_value = state.server_url_input.read(cx).value().trim().to_string();
    validate_url(&url_value)?;
    let email_value = state.email_input.read(cx).value().trim().to_string();
    validate_email(&email_value)?;
    Ok(())
}

/// Open a confirmation dialog before deleting a profile.
///
/// ### Description
/// The Delete button triggers a small modal asking the user to confirm,
/// matching the rest of the app's destructive-action pattern. On confirm,
/// `Fulgur::delete_profile` is called and the parent sheet is closed.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `state`: The shared form state (read for profile id and name).
/// - `window`: The window to attach the dialog to.
/// - `cx`: The application context.
fn confirm_delete_profile(
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

#[cfg(test)]
mod tests {
    use super::{should_warn_for_http_url, validate_email, validate_url};

    #[test]
    fn test_validate_url_accepts_empty() {
        assert!(validate_url("").is_ok());
    }

    #[test]
    fn test_validate_url_rejects_garbage() {
        assert!(validate_url("not-a-url").is_err());
    }

    #[test]
    fn test_validate_url_accepts_https() {
        assert!(validate_url("https://example.com").is_ok());
    }

    #[test]
    fn test_should_warn_for_http_url_accepts_https_without_warning() {
        assert!(!should_warn_for_http_url("https://example.com"));
    }

    #[test]
    fn test_should_warn_for_http_url_warns_for_http() {
        assert!(should_warn_for_http_url("http://example.com"));
    }

    #[test]
    fn test_validate_email_accepts_empty() {
        assert!(validate_email("").is_ok());
    }

    #[test]
    fn test_validate_email_rejects_missing_at() {
        assert!(validate_email("invalid").is_err());
    }

    #[test]
    fn test_validate_email_rejects_missing_dot_after_at() {
        assert!(validate_email("a@b").is_err());
    }

    #[test]
    fn test_validate_email_accepts_simple_address() {
        assert!(validate_email("a@b.c").is_ok());
    }
}
