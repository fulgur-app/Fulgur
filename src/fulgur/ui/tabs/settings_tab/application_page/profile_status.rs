use crate::fulgur::{
    settings::ServerProfile,
    sync::synchronization::{
        FULGURANT_VERSION_WITHOUT_HEADER, RECOMMENDED_FULGURANT_VERSION, SynchronizationStatus,
        VersionCompatibility, compare_required_version,
    },
};
use gpui_kit::App;

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

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_tests {
    use super::{get_profile_status, get_profile_version_display, get_profile_version_warning};
    use crate::fulgur::{
        settings::ServerProfile,
        shared_state::SharedAppState,
        sync::synchronization::{
            RECOMMENDED_FULGURANT_VERSION, SynchronizationStatus, VersionCompatibility,
            compare_required_version,
        },
    };
    use crate::test_support::setup_test_globals;
    use gpui_kit::TestAppContext;

    /// Fulgurant version recent enough for every Fulgur build, written with an
    /// uppercase prefix and padding to exercise the display normalization.
    const RECENT_FULGURANT_HEADER: &str = " V99.1.2 ";

    /// Fulgurant version older than `RECOMMENDED_FULGURANT_VERSION`.
    const OUTDATED_FULGURANT_VERSION: &str = "0.8.0";

    /// Minimum Fulgur version no current build satisfies.
    const FUTURE_FULGUR_VERSION: &str = "99.0.0";

    /// What the profiles table renders for one profile.
    #[derive(Debug, PartialEq, Eq)]
    struct ProfileView {
        status: SynchronizationStatus,
        version: String,
        warning: Option<String>,
    }

    /// Build a profile flagged as active, with its own random id.
    ///
    /// ### Returns
    /// - `ServerProfile`: The active profile.
    fn active_profile() -> ServerProfile {
        let mut profile = ServerProfile::new("Versioned Server");
        profile.is_active = true;
        profile
    }

    /// Seed the sync state of a profile with a connection status and the
    /// versions its server advertised.
    ///
    /// ### Arguments
    /// - `cx`: The test application context.
    /// - `profile`: The profile whose sync state is seeded.
    /// - `status`: The connection status to store.
    /// - `server_version`: The raw `x-fulgurant-version` header, if any.
    /// - `min_fulgur_version`: The minimum Fulgur version the server requires, if any.
    fn seed_sync_state(
        cx: &mut TestAppContext,
        profile: &ServerProfile,
        status: SynchronizationStatus,
        server_version: Option<&str>,
        min_fulgur_version: Option<&str>,
    ) {
        cx.update(|cx| {
            let state = cx.global::<SharedAppState>().sync_state_for(&profile.id);
            *state.connection_status.lock() = status;
            *state.server_version.lock() = server_version.map(str::to_string);
            *state.server_min_fulgur_version.lock() = min_fulgur_version.map(str::to_string);
        });
    }

    /// Resolve everything the profiles table renders for a profile.
    ///
    /// ### Arguments
    /// - `cx`: The test application context.
    /// - `profile`: The profile to resolve.
    /// - `master_on`: Whether the master switch is on.
    ///
    /// ### Returns
    /// - `ProfileView`: The status, version label and warning tooltip.
    fn view_profile(
        cx: &mut TestAppContext,
        profile: &ServerProfile,
        master_on: bool,
    ) -> ProfileView {
        cx.update(|cx| ProfileView {
            status: get_profile_status(profile, master_on, cx),
            version: get_profile_version_display(profile, master_on, cx),
            warning: get_profile_version_warning(profile, master_on, cx),
        })
    }

    /// Build the view of a profile that must not surface any version information.
    ///
    /// ### Arguments
    /// - `status`: The status the profile reports.
    ///
    /// ### Returns
    /// - `ProfileView`: A `"-"` version label and no warning.
    fn hidden_view(status: SynchronizationStatus) -> ProfileView {
        ProfileView {
            status,
            version: "-".to_string(),
            warning: None,
        }
    }

    /// Build the "update Fulgur" warning expected for a server requiring `required`.
    ///
    /// ### Arguments
    /// - `required`: The minimum Fulgur version the server requires.
    ///
    /// ### Returns
    /// - `String`: The tooltip text.
    fn fulgur_update_warning(required: &str) -> String {
        format!(
            "This server needs Fulgur v{required} or newer (you have v{}). Please update Fulgur.",
            env!("CARGO_PKG_VERSION")
        )
    }

    /// Build the "update Fulgurant" warning expected for a pre-0.7.0 server.
    ///
    /// ### Returns
    /// - `String`: The tooltip text.
    fn legacy_fulgurant_warning() -> String {
        format!(
            "This server runs an old Fulgurant (pre-0.7.0). Fulgur works best with v{RECOMMENDED_FULGURANT_VERSION} or newer. Please update Fulgurant."
        )
    }

    #[test]
    fn outdated_fulgurant_fixture_is_below_the_recommended_version() {
        assert_ne!(
            compare_required_version(OUTDATED_FULGURANT_VERSION, RECOMMENDED_FULGURANT_VERSION),
            VersionCompatibility::Compatible
        );
    }

    #[gpui_kit::test]
    fn inactive_or_unknown_profiles_hide_version_and_warnings(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let active = active_profile();
        let inactive = ServerProfile::new("Inactive Server");
        let unknown = active_profile();
        for profile in [&active, &inactive] {
            seed_sync_state(
                cx,
                profile,
                SynchronizationStatus::Connected,
                None,
                Some(FUTURE_FULGUR_VERSION),
            );
        }

        let not_activated = hidden_view(SynchronizationStatus::NotActivated);
        assert_eq!(view_profile(cx, &active, false), not_activated);
        assert_eq!(view_profile(cx, &inactive, true), not_activated);
        assert_eq!(view_profile(cx, &unknown, true), not_activated);
    }

    #[gpui_kit::test]
    fn profiles_not_connected_hide_version_and_warnings(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let profile = active_profile();
        for status in [
            SynchronizationStatus::Connecting,
            SynchronizationStatus::Disconnected,
            SynchronizationStatus::AuthenticationFailed,
            SynchronizationStatus::ConnectionFailed,
            SynchronizationStatus::Other,
        ] {
            seed_sync_state(cx, &profile, status, None, Some(FUTURE_FULGUR_VERSION));
            assert_eq!(view_profile(cx, &profile, true), hidden_view(status));
        }
    }

    #[gpui_kit::test]
    fn connected_profile_normalizes_the_reported_version(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let profile = active_profile();
        seed_sync_state(
            cx,
            &profile,
            SynchronizationStatus::Connected,
            Some(RECENT_FULGURANT_HEADER),
            Some(env!("CARGO_PKG_VERSION")),
        );

        assert_eq!(
            view_profile(cx, &profile, true),
            ProfileView {
                status: SynchronizationStatus::Connected,
                version: "v99.1.2".to_string(),
                warning: None,
            }
        );
    }

    #[gpui_kit::test]
    fn connected_profile_without_header_reports_a_legacy_fulgurant(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let profile = active_profile();
        seed_sync_state(cx, &profile, SynchronizationStatus::Connected, None, None);

        assert_eq!(
            view_profile(cx, &profile, true),
            ProfileView {
                status: SynchronizationStatus::Connected,
                version: "< v0.7.0".to_string(),
                warning: Some(legacy_fulgurant_warning()),
            }
        );
    }

    #[gpui_kit::test]
    fn outdated_fulgurant_warns_with_the_reported_version(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let profile = active_profile();
        let header = format!("v{OUTDATED_FULGURANT_VERSION}");
        seed_sync_state(
            cx,
            &profile,
            SynchronizationStatus::Connected,
            Some(&header),
            None,
        );

        assert_eq!(
            view_profile(cx, &profile, true),
            ProfileView {
                status: SynchronizationStatus::Connected,
                version: header,
                warning: Some(format!(
                    "This server runs Fulgurant v{OUTDATED_FULGURANT_VERSION}, but Fulgur works best with v{RECOMMENDED_FULGURANT_VERSION} or newer. Please update Fulgurant."
                )),
            }
        );
    }

    #[gpui_kit::test]
    fn server_requiring_a_newer_fulgur_warns_to_update_fulgur(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let profile = active_profile();
        seed_sync_state(
            cx,
            &profile,
            SynchronizationStatus::Connected,
            Some(RECENT_FULGURANT_HEADER),
            Some(FUTURE_FULGUR_VERSION),
        );

        assert_eq!(
            view_profile(cx, &profile, true).warning,
            Some(fulgur_update_warning(FUTURE_FULGUR_VERSION))
        );
    }

    #[gpui_kit::test]
    fn both_mismatches_combine_with_the_fulgur_warning_first(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        let profile = active_profile();
        seed_sync_state(
            cx,
            &profile,
            SynchronizationStatus::Connected,
            None,
            Some(FUTURE_FULGUR_VERSION),
        );

        assert_eq!(
            view_profile(cx, &profile, true).warning,
            Some(format!(
                "{}\n{}",
                fulgur_update_warning(FUTURE_FULGUR_VERSION),
                legacy_fulgurant_warning()
            ))
        );
    }
}
