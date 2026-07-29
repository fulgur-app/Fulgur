use super::profile_status::{
    get_profile_status, get_profile_version_display, get_profile_version_warning,
};
use crate::fulgur::{
    Fulgur, settings::ServerProfile, sync::synchronization::SynchronizationStatus,
    ui::icons::CustomIcon,
};
use gpui::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString,
    StatefulInteractiveElement, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme, Icon, Sizable, button::Button, h_flex, label::Label, setting::SettingItem,
    switch::Switch, tooltip::Tooltip, v_flex,
};

/// Render the list of configured profiles as a table-like element.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `SettingItem`: The profiles table row.
pub(super) fn render_profiles_table(entity: &Entity<Fulgur>) -> SettingItem {
    let entity = entity.clone();
    SettingItem::render(move |_options, _window, cx| {
        let profiles = entity
            .read(cx)
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .clone();
        let master_on = entity
            .read(cx)
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated;
        if profiles.is_empty() {
            return v_flex()
                .w_full()
                .gap_1()
                .child(div().text_sm().child("Fulgurant instances"))
                .child(table_header(cx))
                .child(
                    div()
                        .w_full()
                        .p_3()
                        .text_sm()
                        .text_color(cx.theme().muted_foreground)
                        .child("No server configured. Click \"Add server\" below to add your first one."),
                )
                .into_any_element();
        }
        let entity_for_rows = entity.clone();
        v_flex()
            .w_full()
            .gap_1()
            .child(div().text_sm().child("Fulgurant instances"))
            .child(table_header(cx))
            .children(
                profiles
                    .iter()
                    .map(|profile| {
                        render_profile_row(entity_for_rows.clone(), profile, master_on, cx)
                    })
                    .collect::<Vec<_>>(),
            )
            .into_any_element()
    })
}

/// Render the table header for the profiles list.
///
/// ### Arguments
/// - `cx`: The application context (for theme tokens).
///
/// ### Returns
/// - `impl IntoElement`: The header row.
fn table_header(cx: &App) -> impl IntoElement {
    h_flex()
        .w_full()
        .px_2()
        .py_1()
        .gap_2()
        .border_b_1()
        .border_color(cx.theme().border)
        .text_xs()
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(cx.theme().muted_foreground)
        .child(div().flex_1().child("Name"))
        .child(div().w(gpui::px(90.0)).child("Version"))
        .child(div().flex_1().child("URL"))
        .child(div().w(gpui::px(20.0)).child(""))
        .child(div().w(gpui::px(125.0)).child("Status").pr_4())
        .child(div().w(gpui::px(60.0)).child("Activate"))
        .child(div().w(gpui::px(80.0)).child(""))
}

/// Render a single profile row in the profiles table.
///
/// ### Arguments
/// - `entity`: The Fulgur entity (used to open the edit sheet).
/// - `profile`: The profile to render.
/// - `master_on`: Whether the master switch is on.
/// - `cx`: The application context.
///
/// ### Returns
/// - `impl IntoElement`: The row element.
fn render_profile_row(
    entity: Entity<Fulgur>,
    profile: &ServerProfile,
    master_on: bool,
    cx: &App,
) -> impl IntoElement {
    let display_url = profile
        .server_url
        .clone()
        .unwrap_or_else(|| "-".to_string());
    let version_label = get_profile_version_display(profile, master_on, cx);
    let update_warning = get_profile_version_warning(profile, master_on, cx).map(|tooltip| {
        let warn_id = SharedString::from(format!("profile-row-warn-{}", profile.id));
        let tooltip = SharedString::from(tooltip);
        div()
            .id(warn_id)
            .cursor_pointer()
            .child(
                Icon::new(CustomIcon::TriangleAlert)
                    .with_size(gpui::px(18.0))
                    .text_color(cx.theme().warning),
            )
            .tooltip(move |window, cx| Tooltip::new(tooltip.clone()).build(window, cx))
    });
    let pill = render_status_pill(profile, master_on, cx);
    let row_id = SharedString::from(format!("profile-row-{}", profile.id));
    let edit_id = SharedString::from(format!("profile-row-edit-{}", profile.id));
    let activate_id = SharedString::from(format!("profile-row-activate-{}", profile.id));
    let profile_id_for_edit = profile.id.clone();
    let profile_id_for_activate = profile.id.clone();
    let entity_for_activate = entity.clone();
    let is_active = profile.is_active;
    let profile_name = profile.name.clone();
    h_flex()
        .id(row_id)
        .w_full()
        .px_2()
        .py_2()
        .gap_2()
        .items_center()
        .border_b_1()
        .border_color(cx.theme().border)
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(cx.theme().foreground)
                .child(profile_name),
        )
        .child(
            div()
                .w(gpui::px(90.0))
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(version_label),
        )
        .child(
            div()
                .flex_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(display_url),
        )
        .child(
            div()
                .w(gpui::px(20.0))
                .flex()
                .items_center()
                .justify_center()
                .children(update_warning),
        )
        .child(div().w(gpui::px(125.0)).child(pill).pr_4())
        .child(
            div()
                .w(gpui::px(60.0))
                .child(Switch::new(activate_id).checked(is_active).on_click(
                    move |val: &bool, window, cx| {
                        let id = profile_id_for_activate.clone();
                        handle_profile_active_toggle(&entity_for_activate, &id, *val, window, cx);
                    },
                )),
        )
        .child(
            div().w(gpui::px(80.0)).child(
                Button::new(edit_id)
                    .child("Edit")
                    .small()
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        let id = profile_id_for_edit.clone();
                        entity.update(cx, |this, cx| {
                            this.open_edit_profile_sheet(Some(&id), window, cx);
                        });
                    }),
            ),
        )
}

/// Toggle a profile's activation state and refresh its SSE connection.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `profile_id`: The id of the profile to toggle.
/// - `value`: The new activation value.
/// - `window`: The window (used to show connection progress).
/// - `cx`: The application context.
fn handle_profile_active_toggle(
    entity: &Entity<Fulgur>,
    profile_id: &str,
    value: bool,
    window: &mut Window,
    cx: &mut App,
) {
    entity.update(cx, |this, cx| {
        match this.update_profile(profile_id, |profile| profile.is_active = value, cx) {
            Ok(true) => {
                this.restart_sse_connection_for_with_progress(profile_id, window, cx);
            }
            Ok(false) => {
                log::warn!("Profile '{profile_id}' could not be toggled (no longer in settings)");
            }
            Err(e) => {
                log::error!("Failed to toggle profile '{profile_id}': {e}");
            }
        }
    });
}

/// Render a status pill for a profile.
///
/// ### Arguments
/// - `profile`: The profile to render the pill for.
/// - `master_on`: Whether the master switch is on (overrides the per-profile
///   status to Inactive when off).
/// - `cx`: The application context.
///
/// ### Returns
/// - `impl IntoElement`: The pill element.
fn render_status_pill(profile: &ServerProfile, master_on: bool, cx: &App) -> impl IntoElement {
    let status = get_profile_status(profile, master_on, cx);
    let (bg, fg) = pill_colors(status, cx);
    Label::new(status.label())
        .rounded_lg()
        .border_1()
        .border_color(bg)
        .text_sm()
        .text_color(fg)
        .text_center()
}

/// Resolve the foreground/background colors for a status pill.
///
/// ### Arguments
/// - `status`: The profile's status enum value.
///
/// ### Returns
/// - `(gpui::Hsla, gpui::Hsla)`: Background and foreground colors.
fn pill_colors(status: SynchronizationStatus, cx: &App) -> (gpui::Hsla, gpui::Hsla) {
    let theme = cx.theme();
    match status {
        SynchronizationStatus::Connected => (theme.success, theme.success),
        SynchronizationStatus::Connecting => (theme.warning, theme.warning),
        SynchronizationStatus::AuthenticationFailed
        | SynchronizationStatus::ConnectionFailed
        | SynchronizationStatus::NotActivated
        | SynchronizationStatus::Other => (theme.danger, theme.danger),
        SynchronizationStatus::Disconnected => (theme.info, theme.info),
    }
}
