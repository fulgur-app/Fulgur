use crate::fulgur::{Fulgur, settings::MAX_PROFILES, utils::crypto_helper};
use gpui::{
    App, Entity, FontWeight, IntoElement, ParentElement, SharedString, Styled, Window, div,
};
use gpui_component::{
    ActiveTheme, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    setting::SettingItem,
    switch::Switch,
    v_flex,
};

/// Render the inline error banner shown when key initialization failed.
///
/// ### Returns
/// - `SettingItem`: The banner element wrapped as a `SettingItem`.
pub(super) fn render_sync_error_banner() -> SettingItem {
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
pub(super) fn render_master_switch(entity: &Entity<Fulgur>) -> SettingItem {
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

/// Render the right-aligned "Add server" button.
///
/// ### Arguments
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `SettingItem`: The button row.
pub(super) fn render_add_server_button(entity: &Entity<Fulgur>) -> SettingItem {
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
