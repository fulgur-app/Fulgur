mod form_state;
mod handlers;
mod render;
mod validation;

use crate::fulgur::{
    Fulgur,
    settings::{MAX_PROFILES, new_profile_id},
};
use form_state::{DEVICE_KEY_PLACEHOLDER, ProfileFormState};
use gpui::{AppContext, Context, ParentElement, SharedString, Styled, Window, div, px};
use gpui_component::{
    Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    input::InputState,
    notification::NotificationType,
    v_flex,
};
use handlers::{confirm_delete_profile, handle_cancel, handle_save, handle_test_connection};
use parking_lot::Mutex;
use render::render_form_body;
use std::sync::{Arc, atomic::AtomicBool};

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
