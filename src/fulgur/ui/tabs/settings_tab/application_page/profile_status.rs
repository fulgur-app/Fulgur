use crate::fulgur::{
    settings::ServerProfile,
    sync::synchronization::{
        FULGURANT_VERSION_WITHOUT_HEADER, RECOMMENDED_FULGURANT_VERSION, SynchronizationStatus,
        VersionCompatibility, compare_required_version,
    },
};
use gpui::App;

/// Get the status  for a profile.
///
/// ### Arguments
/// - `profile`: The profile.
/// - `master_on`: The master switch value.
/// - `cx`: The application context.
///
/// ### Returns
/// - `SynchronizationStatus`: The profile's status.
pub(super) fn get_profile_status(
    profile: &ServerProfile,
    master_on: bool,
    cx: &App,
) -> SynchronizationStatus {
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

/// Resolve the version label to display for a profile.
///
/// ### Arguments
/// - `profile`: The profile to resolve the version for.
/// - `master_on`: Whether the master switch is on.
/// - `cx`: The application context.
///
/// ### Returns
/// - `String`: The version label to render.
pub(super) fn get_profile_version_display(
    profile: &ServerProfile,
    master_on: bool,
    cx: &App,
) -> String {
    if !matches!(
        get_profile_status(profile, master_on, cx),
        SynchronizationStatus::Connected
    ) {
        return "-".to_string();
    }
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_states = shared.sync_states.read();
    let Some(state) = sync_states.get(&profile.id) else {
        return "-".to_string();
    };
    let version = state.server_version.lock();
    match version.as_deref() {
        Some(raw) => format!("v{}", raw.trim().trim_start_matches(['v', 'V'])),
        None => "< v0.7.0".to_string(),
    }
}

/// Resolve an "update Fulgur" warning for a profile when its server requires a
/// newer Fulgur version than the running build.
///
/// ### Arguments
/// - `profile`: The profile to check.
/// - `master_on`: Whether the master switch is on.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Some(String)`: A tooltip describing the required Fulgur update.
/// - `None`: The running Fulgur is compatible, or the profile is not connected.
fn get_profile_fulgur_update_warning(
    profile: &ServerProfile,
    master_on: bool,
    cx: &App,
) -> Option<String> {
    if !matches!(
        get_profile_status(profile, master_on, cx),
        SynchronizationStatus::Connected
    ) {
        return None;
    }
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_states = shared.sync_states.read();
    let required = sync_states
        .get(&profile.id)?
        .server_min_fulgur_version
        .lock()
        .clone()?;
    let current = env!("CARGO_PKG_VERSION");
    match compare_required_version(current, &required) {
        VersionCompatibility::Compatible => None,
        VersionCompatibility::UpdateRecommended | VersionCompatibility::UpdateRequired => {
            Some(format!(
                "This server needs Fulgur v{required} or newer (you have v{current}). Please update Fulgur."
            ))
        }
    }
}

/// Resolve an "update Fulgurant" warning for a profile when the connected
/// Fulgurant is older than the version this Fulgur build is best paired with.
///
/// ### Arguments
/// - `profile`: The profile to check.
/// - `master_on`: Whether the master switch is on.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Some(String)`: A tooltip describing the recommended Fulgurant update.
/// - `None`: Fulgurant is recent enough, or the profile is not connected.
fn get_profile_fulgurant_update_warning(
    profile: &ServerProfile,
    master_on: bool,
    cx: &App,
) -> Option<String> {
    if !matches!(
        get_profile_status(profile, master_on, cx),
        SynchronizationStatus::Connected
    ) {
        return None;
    }
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_states = shared.sync_states.read();
    let reported = sync_states.get(&profile.id)?.server_version.lock().clone();
    let current = reported.as_deref().map_or_else(
        || FULGURANT_VERSION_WITHOUT_HEADER.to_string(),
        |raw| raw.trim().trim_start_matches(['v', 'V']).to_string(),
    );
    let required = RECOMMENDED_FULGURANT_VERSION;
    match compare_required_version(&current, required) {
        VersionCompatibility::Compatible => None,
        VersionCompatibility::UpdateRecommended | VersionCompatibility::UpdateRequired => {
            Some(match reported {
                Some(_) => format!(
                    "This server runs Fulgurant v{current}, but Fulgur works best with v{required} or newer. Please update Fulgurant."
                ),
                None => format!(
                    "This server runs an old Fulgurant (pre-0.7.0). Fulgur works best with v{required} or newer. Please update Fulgurant."
                ),
            })
        }
    }
}

/// Resolve the combined version-mismatch warning for a profile, covering both
/// "update Fulgur" (server too new) and "update Fulgurant" (server too old).
///
/// ### Arguments
/// - `profile`: The profile to check.
/// - `master_on`: Whether the master switch is on.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Some(String)`: A tooltip with one or both warnings, newline-separated.
/// - `None`: No version mismatch to surface.
pub(super) fn get_profile_version_warning(
    profile: &ServerProfile,
    master_on: bool,
    cx: &App,
) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(warning) = get_profile_fulgur_update_warning(profile, master_on, cx) {
        parts.push(warning);
    }
    if let Some(warning) = get_profile_fulgurant_update_warning(profile, master_on, cx) {
        parts.push(warning);
    }
    (!parts.is_empty()).then(|| parts.join("\n"))
}
