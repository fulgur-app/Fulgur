use crate::fulgur::{
    Fulgur,
    settings::{AppSettings, MAX_PROFILES, ServerProfile},
    sync::synchronization::SynchronizationStatus,
    utils::crypto_helper,
};
use gpui::{
    App, Entity, FontWeight, InteractiveElement, IntoElement, ParentElement, SharedString, Styled,
    Window, div,
};
use gpui_component::{
    ActiveTheme, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    label::Label,
    setting::{SettingField, SettingGroup, SettingItem, SettingPage},
    switch::Switch,
    v_flex,
};

/// Create the Application settings page
///
/// ### Arguments
/// - `entity`: The Fulgur entity
///
/// ### Returns
/// - `SettingPage`: The Application settings page
pub fn create_application_page(entity: &Entity<Fulgur>) -> SettingPage {
    let default_app_settings = AppSettings::new();

    SettingPage::new("Application")
        .default_open(true)
        .groups(vec![
            SettingGroup::new().title("General").items(vec![
                SettingItem::new(
                    "Confirm Exit",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.app_settings.confirm_exit
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.app_settings.confirm_exit = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {e}");
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.confirm_exit),
                )
                .description("Show confirmation dialog before exiting the application."),
                SettingItem::new(
                    "Debug mode",
                    SettingField::switch(
                        {
                            let entity = entity.clone();
                            move |cx: &App| entity.read(cx).settings.app_settings.debug_mode
                        },
                        {
                            let entity = entity.clone();
                            move |val: bool, cx: &mut App| {
                                entity.update(cx, |this, cx| {
                                    this.settings.app_settings.debug_mode = val;
                                    if let Err(e) = this.update_and_propagate_settings(cx) {
                                        log::error!("Failed to save settings: {e}");
                                    }
                                });
                            }
                        },
                    )
                    .default_value(default_app_settings.debug_mode),
                )
                .description("Enables debug mode, showing more info in the logs."),
            ]),
            SettingGroup::new().title("Synchronization").items(vec![
                render_sync_error_banner(),
                render_master_switch(entity),
                render_profiles_table(entity),
                render_add_server_button(entity),
            ]),
        ])
}

/// Render the inline error banner shown when key initialization failed.
///
/// ### Returns
/// - `SettingItem`: The banner element wrapped as a `SettingItem`.
fn render_sync_error_banner() -> SettingItem {
    SettingItem::render(move |_options, _window, cx| {
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        if let Some(error_msg) = shared.sync_error.lock().as_ref() {
            v_flex()
                .w_full()
                .p_3()
                .mb_2()
                .bg(cx.theme().muted)
                .border_1()
                .border_color(cx.theme().border)
                .rounded(gpui::px(4.0))
                .child(
                    div()
                        .text_color(cx.theme().foreground)
                        .text_size(gpui::px(13.0))
                        .font_weight(FontWeight::SEMIBOLD)
                        .child(error_msg.clone()),
                )
                .into_any_element()
        } else {
            div().into_any_element()
        }
    })
}

/// Render the master "Activate sharing" switch.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `SettingItem`: The master switch row.
fn render_master_switch(entity: &Entity<Fulgur>) -> SettingItem {
    let entity = entity.clone();
    SettingItem::render(move |_options, _window, cx| {
        let is_activated = entity
            .read(cx)
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated;
        h_flex()
            .w_full()
            .justify_between()
            .items_start()
            .gap_3()
            .child(
                v_flex()
                    .flex_1()
                    .max_w_3_5()
                    .gap_1()
                    .child(div().text_sm().child("Activate sharing"))
                    .child(
                        div()
                            .text_sm()
                            .text_color(cx.theme().muted_foreground)
                            .child("Master switch for synchronization. When off, all connections to Fulgurant instances are disabled."),
                    ),
            )
            .child(
                Switch::new("activate-sync-master-switch")
                    .checked(is_activated)
                    .on_click({
                        let entity = entity.clone();
                        move |val: &bool, _window: &mut Window, cx: &mut App| {
                            handle_master_toggle(&entity, *val, cx);
                        }
                    }),
            )
            .into_any_element()
    })
}

/// Apply a master switch toggle and propagate the side effects.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
/// - `value`: The new master switch value.
/// - `cx`: The application context.
fn handle_master_toggle(entity: &Entity<Fulgur>, value: bool, cx: &mut App) {
    let active_profile_ids = entity.update(cx, |this, cx| {
        this.settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated = value;
        if value && let Err(e) = crypto_helper::check_private_public_keys(&mut this.settings) {
            log::error!("Failed to check private/public keys: {e}");
        }
        if let Err(e) = this.update_and_propagate_settings(cx) {
            log::error!("Failed to save settings: {e}");
        }
        let ids: Vec<String> = this
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .filter(|p| p.is_active)
            .map(|p| p.id.clone())
            .collect();
        for profile_id in &ids {
            this.restart_sse_connection_for(profile_id, cx);
        }
        ids
    });
    log::debug!(
        "Master switch toggled to {value}; refreshed SSE for {} active profile(s)",
        active_profile_ids.len()
    );
}

/// Render the list of configured profiles as a table-like element.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `SettingItem`: The profiles table row.
fn render_profiles_table(entity: &Entity<Fulgur>) -> SettingItem {
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
        .child(div().flex_1().child("URL"))
        .child(div().w(gpui::px(110.0)).child("Status"))
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
    let pill = render_status_pill(profile, master_on, cx);
    let row_id = SharedString::from(format!("profile-row-{}", profile.id));
    let edit_id = SharedString::from(format!("profile-row-edit-{}", profile.id));
    let profile_id_for_edit = profile.id.clone();
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
                .flex_1()
                .text_sm()
                .text_color(cx.theme().muted_foreground)
                .child(display_url),
        )
        .child(div().w(gpui::px(110.0)).child(pill))
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

/// Get the status  for a profile.
///
/// ### Arguments
/// - `profile`: The profile.
/// - `master_on`: The master switch value.
/// - `cx`: The application context.
///
/// ### Returns
/// - `SynchronizationStatus`: The profile's status.
fn get_profile_status(profile: &ServerProfile, master_on: bool, cx: &App) -> SynchronizationStatus {
    if !master_on || !profile.is_active {
        return SynchronizationStatus::NotActivated;
    }
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_states = shared.sync_states.read();
    sync_states
        .get(&profile.id)
        .map_or(SynchronizationStatus::NotActivated, |state| {
            *state.connection_status.lock()
        })
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

/// Render the right-aligned "Add server" button.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `SettingItem`: The button row.
fn render_add_server_button(entity: &Entity<Fulgur>) -> SettingItem {
    let entity = entity.clone();
    SettingItem::render(move |_options, _window, _cx| {
        let entity = entity.clone();
        h_flex()
            .w_full()
            .justify_end()
            .mt_2()
            .child(
                Button::new("add-server-profile")
                    .child("Add Fulgurant instance")
                    .small()
                    .primary()
                    .cursor_pointer()
                    .on_click(move |_, window, cx| {
                        let already_at_cap = entity
                            .read(cx)
                            .settings
                            .app_settings
                            .synchronization_settings
                            .profiles
                            .len()
                            >= MAX_PROFILES;
                        if already_at_cap {
                            window.push_notification(
                                (
                                    gpui_component::notification::NotificationType::Error,
                                    SharedString::from(format!(
                                        "Maximum of {MAX_PROFILES} Fulgurant instances reached."
                                    )),
                                ),
                                cx,
                            );
                            return;
                        }
                        entity.update(cx, |this, cx| {
                            this.open_edit_profile_sheet(None, window, cx);
                        });
                    }),
            )
            .into_any_element()
    })
}
