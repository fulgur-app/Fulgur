use super::{AppNotification, SharedAppState, SyncState};
use crate::fulgur::settings::ProfileId;
use crate::fulgur::state::{RemoteHistory, StateDb};
use crate::fulgur::utils::crypto_helper::ProfileKeyFailure;
use crate::fulgur::{
    settings::Settings, settings::Themes, sync::synchronization::SynchronizationStatus,
};
use futures::channel::mpsc::UnboundedSender;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

/// Outcome of the encryption key check run when the app starts.
#[derive(Debug, Default)]
pub(super) struct StartupKeyCheck {
    /// Active profiles whose keypair could not be ensured.
    pub(super) failed_profiles: HashSet<ProfileId>,
    /// User-facing description of what failed, `None` when every check passed.
    pub(super) error: Option<String>,
}

impl SharedAppState {
    /// Read the remote URL history persisted in the session-state database.
    ///
    /// ### Arguments
    /// - `state_db`: The session-state database, `None` when none could be opened
    ///
    /// ### Returns
    /// - `RemoteHistory`: The persisted history, empty when there is no database
    ///   or when it cannot be read
    pub(super) fn load_remote_history(state_db: Option<&StateDb>) -> RemoteHistory {
        let Some(state_db) = state_db else {
            return RemoteHistory::default();
        };
        state_db.load_remote_history().unwrap_or_else(|e| {
            log::error!("Failed to load the remote URL history: {e}");
            RemoteHistory::default()
        })
    }

    /// Ensure the active profiles have encryption keys before synchronization starts.
    ///
    /// ### Arguments
    /// - `settings`: Application settings; active profiles whose public key had to
    ///   be generated or recovered are updated in place
    /// - `check_keys`: Ensures the active profiles have keypairs, called only
    ///   when synchronization is activated
    ///
    /// ### Returns
    /// - `StartupKeyCheck`: The profiles whose keys failed and the message to show the user
    pub(super) fn check_startup_keys(
        settings: &mut Settings,
        check_keys: impl FnOnce(&mut Settings) -> anyhow::Result<Vec<ProfileKeyFailure>>,
    ) -> StartupKeyCheck {
        if !settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            return StartupKeyCheck::default();
        }
        match check_keys(settings) {
            Ok(failures) if failures.is_empty() => StartupKeyCheck::default(),
            Ok(failures) => {
                let details = failures
                    .iter()
                    .map(|failure| format!("{}: {}", failure.profile_name, failure.error))
                    .collect::<Vec<_>>()
                    .join("; ");
                StartupKeyCheck {
                    failed_profiles: failures
                        .into_iter()
                        .map(|failure| failure.profile_id)
                        .collect(),
                    error: Some(format!(
                        "Failed to initialize encryption keys. The affected servers stay disconnected until you reconnect them or restart Fulgur. Error: {details}"
                    )),
                }
            }
            Err(e) => {
                let error_msg = format!("Failed to save the generated encryption keys: {e}");
                log::error!("{error_msg}");
                StartupKeyCheck {
                    failed_profiles: HashSet::new(),
                    error: Some(error_msg),
                }
            }
        }
    }

    /// Load themes from disk
    ///
    /// ### Returns
    /// - `Some(Themes)`: The loaded themes
    /// - `None`: If themes cannot be loaded
    pub(super) fn load_themes() -> Option<Themes> {
        Themes::load().ok()
    }

    /// Determine the initial synchronization status based on settings
    ///
    /// ### Arguments
    /// - `settings`: The application settings
    ///
    /// ### Returns
    /// - `SynchronizationStatus`: Connecting if sync is activated, `NotActivated` otherwise
    pub(super) fn determine_initial_sync_status(settings: &Settings) -> SynchronizationStatus {
        if settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            SynchronizationStatus::Connecting
        } else {
            SynchronizationStatus::NotActivated
        }
    }

    /// Build the initial per-profile `SyncState` map from settings.
    ///
    /// ### Arguments
    /// - `settings`: The application settings.
    /// - `default_status`: The status assigned to profiles flagged as active
    ///   when the master switch is on; ignored otherwise.
    /// - `key_failures`: Active profiles whose encryption keys could not be
    ///   ensured, seeded as disconnected instead of `default_status`.
    /// - `notification_tx`: The app-wide notification sender cloned into each state.
    ///
    /// ### Returns
    /// - `HashMap<ProfileId, Arc<SyncState>>`: Map keyed by profile id.
    pub(super) fn seed_sync_states(
        settings: &Settings,
        default_status: SynchronizationStatus,
        key_failures: &HashSet<ProfileId>,
        notification_tx: &UnboundedSender<AppNotification>,
    ) -> HashMap<ProfileId, Arc<SyncState>> {
        let master_on = settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated;
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .map(|profile| {
                let status = if !master_on || !profile.is_active {
                    SynchronizationStatus::NotActivated
                } else if key_failures.contains(&profile.id) {
                    SynchronizationStatus::Disconnected
                } else {
                    default_status
                };
                (
                    profile.id.clone(),
                    Arc::new(SyncState::new(status, notification_tx.clone())),
                )
            })
            .collect()
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::*;
    use crate::fulgur::settings::ServerProfile;
    use futures::channel::mpsc::unbounded;

    /// Build settings whose synchronization master switch has the given value.
    ///
    /// ### Arguments
    /// - `sync_activated`: The value of the synchronization master switch.
    ///
    /// ### Returns
    /// - `Settings`: Default settings with the master switch set.
    fn settings_with_sync(sync_activated: bool) -> Settings {
        let mut settings = Settings::new();
        settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated = sync_activated;
        settings
    }

    /// Build a key failure report for a profile.
    ///
    /// ### Arguments
    /// - `profile`: The profile whose keys failed.
    /// - `error`: The error text to carry.
    ///
    /// ### Returns
    /// - `ProfileKeyFailure`: The failure report.
    fn key_failure(profile: &ServerProfile, error: &str) -> ProfileKeyFailure {
        ProfileKeyFailure {
            profile_id: profile.id.clone(),
            profile_name: profile.name.clone(),
            error: anyhow::anyhow!(error.to_string()),
        }
    }

    /// Read the master switch of the given settings.
    ///
    /// ### Arguments
    /// - `settings`: The settings to read.
    ///
    /// ### Returns
    /// - `bool`: The value of the synchronization master switch.
    fn master_switch(settings: &Settings) -> bool {
        settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
    }

    #[test]
    fn test_check_startup_keys_skips_key_check_when_sync_is_off() {
        let mut settings = settings_with_sync(false);
        let key_check = SharedAppState::check_startup_keys(&mut settings, |_| {
            panic!("the key check must not run while synchronization is off")
        });
        assert!(key_check.error.is_none());
        assert!(key_check.failed_profiles.is_empty());
        assert!(!master_switch(&settings));
    }

    #[test]
    fn test_check_startup_keys_reports_nothing_when_keys_are_valid() {
        let mut settings = settings_with_sync(true);
        let key_check = SharedAppState::check_startup_keys(&mut settings, |_| Ok(Vec::new()));
        assert!(key_check.error.is_none());
        assert!(key_check.failed_profiles.is_empty());
        assert!(master_switch(&settings));
    }

    #[test]
    fn test_check_startup_keys_keeps_sync_on_and_reports_failing_profiles() {
        let mut settings = settings_with_sync(true);
        let locked = ServerProfile::new("Locked");
        let healthy = ServerProfile::new("Healthy");
        settings.app_settings.synchronization_settings.profiles =
            vec![locked.clone(), healthy.clone()];

        let key_check = SharedAppState::check_startup_keys(&mut settings, |_| {
            Ok(vec![key_failure(&locked, "keychain locked")])
        });

        assert!(
            master_switch(&settings),
            "a key failure must never switch synchronization off, or the next save persists it"
        );
        assert_eq!(
            key_check.failed_profiles,
            HashSet::from([locked.id.clone()])
        );
        let message = key_check
            .error
            .expect("a key failure must produce a user-facing message");
        assert!(
            message.contains("Locked: keychain locked"),
            "the message must name the profile and carry the underlying error: {message}"
        );
        assert!(!message.contains("Healthy"));
    }

    #[test]
    fn test_check_startup_keys_reports_save_failure_without_skipping_profiles() {
        let mut settings = settings_with_sync(true);
        let key_check = SharedAppState::check_startup_keys(&mut settings, |_| {
            Err(anyhow::anyhow!("disk full"))
        });
        assert!(master_switch(&settings));
        assert!(
            key_check.failed_profiles.is_empty(),
            "the keys exist in the keychain, only their persistence failed"
        );
        let message = key_check.error.expect("a save failure must be reported");
        assert!(message.contains("disk full"));
    }

    #[test]
    fn test_seed_sync_states_leaves_key_failures_disconnected() {
        let mut settings = settings_with_sync(true);
        let mut locked = ServerProfile::new("Locked");
        locked.is_active = true;
        let mut healthy = ServerProfile::new("Healthy");
        healthy.is_active = true;
        settings.app_settings.synchronization_settings.profiles =
            vec![locked.clone(), healthy.clone()];
        let (notification_tx, _notification_rx) = unbounded();

        let sync_states = SharedAppState::seed_sync_states(
            &settings,
            SynchronizationStatus::Connecting,
            &HashSet::from([locked.id.clone()]),
            &notification_tx,
        );

        assert_eq!(
            *sync_states[&locked.id].connection_status.lock(),
            SynchronizationStatus::Disconnected
        );
        assert_eq!(
            *sync_states[&healthy.id].connection_status.lock(),
            SynchronizationStatus::Connecting
        );
    }
}
