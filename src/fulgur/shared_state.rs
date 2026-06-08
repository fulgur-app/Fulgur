use crate::fulgur::settings::ProfileId;
use crate::fulgur::state_writer::StateWriter;
use crate::fulgur::sync::ssh::credentials::SshCredentialCache;
use crate::fulgur::sync::ssh::pool::SshSessionPool;
use crate::fulgur::utils::crypto_helper::{
    check_private_public_keys, migrate_legacy_keychain_entries_if_present,
};
use crate::fulgur::utils::updater::UpdateInfo;
use crate::fulgur::{
    settings::Settings, settings::Themes, sync::synchronization::SynchronizationStatus,
};
use fulgur_common::api::shares::SharedFileResponse;
use gpui::SharedString;
use gpui_component::notification::NotificationType;
use parking_lot::{Mutex, RwLock};
use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

/// Result of a background device fetch: either a list of devices or an error message,
/// paired with a boolean indicating whether SSE reconnection is needed.
pub type PendingDevicesResult = (
    Result<Vec<fulgur_common::api::devices::DeviceResponse>, String>,
    bool,
);

/// Sync-related state for a single profile, shared across all windows.
pub struct SyncState {
    /// Sync server connection status for this profile.
    pub connection_status: Arc<Mutex<SynchronizationStatus>>,
    /// Timestamp when the connection attempt started (for delayed spinner display).
    pub connecting_since: Arc<Mutex<Option<std::time::Instant>>>,
    /// Device name reported by the server for this profile.
    pub device_name: Arc<Mutex<Option<String>>>,
    /// Pending shared files arrived from this profile's server.
    pub pending_shared_files: Arc<Mutex<Vec<SharedFileResponse>>>,
    /// JWT token state manager with condition variable for efficient token
    /// refresh coordination, scoped to this profile.
    pub token_state: Arc<crate::fulgur::sync::access_token::TokenStateManager>,
    /// Last heartbeat time received from this profile's SSE stream.
    pub last_heartbeat: Arc<Mutex<Option<std::time::Instant>>>,
    /// Pending notification from background sync operations (checked in render loop).
    pub pending_notification: Arc<Mutex<Option<(NotificationType, SharedString)>>>,
    /// Last emitted receive-error signature for shared-file processing (error deduplication).
    pub last_share_receive_error_signature: Arc<Mutex<Option<String>>>,
    /// Pending devices list from background fetch (checked in render loop to open share sheet).
    pub pending_devices: Arc<Mutex<Option<PendingDevicesResult>>>,
    /// Maximum file size for sharing (bytes), as reported by this profile's server.
    pub max_file_size_bytes: Arc<AtomicU64>,
}

impl SyncState {
    /// Create a new per-profile sync state with the given connection status.
    ///
    /// ### Arguments
    /// - `connection_status`: The initial sync server connection status.
    ///
    /// ### Returns
    /// - `Self`: The new sync state.
    pub fn new(connection_status: SynchronizationStatus) -> Self {
        Self {
            connection_status: Arc::new(Mutex::new(connection_status)),
            connecting_since: Arc::new(Mutex::new(None)),
            device_name: Arc::new(Mutex::new(None)),
            pending_shared_files: Arc::new(Mutex::new(Vec::new())),
            token_state: Arc::new(crate::fulgur::sync::access_token::TokenStateManager::new()),
            last_heartbeat: Arc::new(Mutex::new(None)),
            pending_notification: Arc::new(Mutex::new(None)),
            last_share_receive_error_signature: Arc::new(Mutex::new(None)),
            pending_devices: Arc::new(Mutex::new(None)),
            max_file_size_bytes: Arc::new(AtomicU64::new(u64::MAX)),
        }
    }
}

/// State that is shared across all windows. This includes settings, themes, and sync-related state.
pub struct SharedAppState {
    /// Settings (shared across all windows)
    pub settings: Arc<Mutex<Settings>>,
    /// Settings version counter, incremented whenever settings change. All windows check this to detect when they need to reload settings.
    pub settings_version: Arc<AtomicU64>,
    /// Available themes
    pub themes: Arc<Mutex<Option<Themes>>>,
    /// Per-profile sync states keyed by profile id.
    pub sync_states: Arc<RwLock<HashMap<ProfileId, Arc<SyncState>>>>,
    /// Global flag to ensure sync bootstrap runs only once across all windows.
    pub sync_initialized: Arc<AtomicBool>,
    /// Synchronization initialization error (if key generation failed)
    pub sync_error: Arc<Mutex<Option<String>>>,
    /// Update info if available
    pub update_info: Arc<Mutex<Option<UpdateInfo>>>,
    /// Files from macOS "Open with" events (already `Arc<Mutex>`)
    pub pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>,
    /// Pending IPC commands from Windows jump list ("new-tab", "new-window")
    #[cfg(target_os = "windows")]
    pub pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    /// Shared HTTP agent for connection pooling across all short-lived REST
    /// requests (token, ping, share fetch). Carries a 10s global timeout.
    pub http_agent: Arc<ureq::Agent>,
    /// Dedicated HTTP agent for the long-lived SSE stream only.
    pub sse_http_agent: Arc<ureq::Agent>,
    /// Session-scoped SSH password cache keyed by `(host, port, user)`.
    ///
    /// The cache is memory-only and dropped on app exit.
    pub ssh_session_cache: Arc<Mutex<SshCredentialCache>>,
    /// Process-wide pool of authenticated SSH sessions used to amortize TCP +
    /// SSH handshakes across successive remote operations.
    pub ssh_session_pool: Arc<SshSessionPool>,
    /// Dedicated background writer for `WindowsState` persistence.
    pub state_writer: Arc<StateWriter>,
}

impl gpui::Global for SharedAppState {}

impl SharedAppState {
    /// Create a new shared app state by orchestrating the initialization of shared application state
    /// by loading themes, and determining the initial synchronization status.
    ///
    /// ### Arguments
    /// - `settings`: Already-loaded application settings
    /// - `pending_files_from_macos`: Arc to the pending files queue from macOS open events
    ///
    /// ### Returns
    /// - `Self`: The new shared app state
    pub fn new(settings: Settings, pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>) -> Self {
        let (settings, sync_error) = Self::validate_settings(settings);
        let themes = Self::load_themes();
        let synchronization_status = Self::determine_initial_sync_status(&settings);
        let sync_states = Self::seed_sync_states(&settings, synchronization_status);

        Self {
            settings: Arc::new(Mutex::new(settings)),
            settings_version: Arc::new(AtomicU64::new(0)),
            themes: Arc::new(Mutex::new(themes)),
            sync_states: Arc::new(RwLock::new(sync_states)),
            sync_initialized: Arc::new(AtomicBool::new(false)),
            sync_error: Arc::new(Mutex::new(sync_error)),
            update_info: Arc::new(Mutex::new(None)),
            pending_files_from_macos,
            #[cfg(target_os = "windows")]
            pending_ipc_commands: Arc::new(Mutex::new(Vec::new())),
            http_agent: Arc::new(ureq::Agent::new_with_config(
                ureq::config::Config::builder()
                    .timeout_connect(Some(std::time::Duration::from_secs(5)))
                    .timeout_global(Some(std::time::Duration::from_secs(10)))
                    .build(),
            )),
            sse_http_agent: Arc::new(ureq::Agent::new_with_config(
                ureq::config::Config::builder()
                    .timeout_connect(Some(std::time::Duration::from_secs(5)))
                    .timeout_global(Some(std::time::Duration::from_secs(90)))
                    .build(),
            )),
            ssh_session_cache: Arc::new(Mutex::new(SshCredentialCache::new())),
            ssh_session_pool: Arc::new(SshSessionPool::new()),
            state_writer: Arc::new(StateWriter::new()),
        }
    }

    /// Validate encryption keys against pre-loaded settings.
    ///
    /// If synchronization is activated but keys cannot be validated, disables
    /// synchronization and returns the error message for user notification.
    ///
    /// ### Arguments
    /// - `settings`: Application settings to validate
    ///
    /// ### Returns
    /// - `(Settings, Option<String>)`: The validated settings and an optional error message
    ///   if key validation failed
    fn validate_settings(mut settings: Settings) -> (Settings, Option<String>) {
        if let Err(e) = migrate_legacy_keychain_entries_if_present(&settings) {
            //TODO: remove in 0.10.0
            log::warn!("Legacy keychain migration failed: {e}");
        }
        let sync_error = if settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            match check_private_public_keys(&mut settings) {
                Ok(()) => None,
                Err(e) => {
                    let error_msg = format!(
                        "Failed to initialize encryption keys. Synchronization has been disabled. Error: {e}"
                    );
                    log::error!("{error_msg}");
                    settings
                        .app_settings
                        .synchronization_settings
                        .is_synchronization_activated = false;
                    Some(error_msg)
                }
            }
        } else {
            None
        };
        (settings, sync_error)
    }

    /// Load themes from disk
    ///
    /// ### Returns
    /// - `Some(Themes)`: The loaded themes
    /// - `None`: If themes cannot be loaded
    fn load_themes() -> Option<Themes> {
        Themes::load().ok()
    }

    /// Determine the initial synchronization status based on settings
    ///
    /// ### Arguments
    /// - `settings`: The application settings
    ///
    /// ### Returns
    /// - `SynchronizationStatus`: Connecting if sync is activated, `NotActivated` otherwise
    fn determine_initial_sync_status(settings: &Settings) -> SynchronizationStatus {
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
    ///
    /// ### Returns
    /// - `HashMap<ProfileId, Arc<SyncState>>`: Map keyed by profile id.
    fn seed_sync_states(
        settings: &Settings,
        default_status: SynchronizationStatus,
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
                let status = if master_on && profile.is_active {
                    default_status
                } else {
                    SynchronizationStatus::NotActivated
                };
                (profile.id.clone(), Arc::new(SyncState::new(status)))
            })
            .collect()
    }

    /// Get the `SyncState` for a specific profile, creating it on demand.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile id.
    ///
    /// ### Returns
    /// - `Arc<SyncState>`: The shared sync state for the profile. A fresh
    ///   `NotActivated` state is inserted when the profile is unknown.
    pub fn sync_state_for(&self, profile_id: &str) -> Arc<SyncState> {
        if let Some(existing) = self.sync_states.read().get(profile_id) {
            return Arc::clone(existing);
        }
        let mut map = self.sync_states.write();
        Arc::clone(
            map.entry(profile_id.to_string())
                .or_insert_with(|| Arc::new(SyncState::new(SynchronizationStatus::NotActivated))),
        )
    }

    /// Get the `SyncState` for the first configured profile.
    ///
    /// ### Returns
    /// - `Arc<SyncState>`: The first profile's sync state, or the
    ///   empty-id-keyed fallback state when there are no profiles.
    pub fn primary_sync_state(&self) -> Arc<SyncState> {
        let primary_id = self
            .settings
            .lock()
            .app_settings
            .synchronization_settings
            .profiles
            .first()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.sync_state_for(&primary_id)
    }

    /// Remove the `SyncState` entry for a profile and return it.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile id whose state should be dropped.
    ///
    /// ### Returns
    /// - `Some(Arc<SyncState>)`: The removed state if it existed.
    /// - `None`: When no entry exists for the profile.
    pub fn remove_sync_state(&self, profile_id: &str) -> Option<Arc<SyncState>> {
        self.sync_states.write().remove(profile_id)
    }
}
