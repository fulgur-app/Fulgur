use super::form_state::ProfileFormState;
use gpui::{App, FontWeight, IntoElement, ParentElement, Styled, div, prelude::FluentBuilder};
use gpui_component::{ActiveTheme, h_flex, input::Input, switch::Switch, v_flex};
use std::sync::Arc;

/// Render the body of the edit profile sheet.
///
/// ### Arguments
/// - `state`: The shared form state.
/// - `cx`: The application context (used to read theme tokens).
///
/// ### Returns
/// - `impl IntoElement`: The form body, ready to be attached to the sheet.
pub(super) fn render_form_body(state: &Arc<ProfileFormState>, cx: &App) -> impl IntoElement {
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
