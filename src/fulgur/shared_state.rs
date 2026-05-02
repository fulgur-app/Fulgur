use crate::fulgur::state_writer::StateWriter;
use crate::fulgur::sync::ssh::credentials::SshCredentialCache;
use crate::fulgur::sync::ssh::pool::SshSessionPool;
use crate::fulgur::utils::crypto_helper::check_private_public_keys;
use crate::fulgur::utils::updater::UpdateInfo;
use crate::fulgur::{
    settings::Settings, settings::Themes, sync::synchronization::SynchronizationStatus,
};
use fulgur_common::api::shares::SharedFileResponse;
use gpui::SharedString;
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

/// Result of a background device fetch: either a list of devices or an error message,
/// paired with a boolean indicating whether SSE reconnection is needed.
pub type PendingDevicesResult = (
    Result<Vec<fulgur_common::api::devices::DeviceResponse>, String>,
    bool,
);

/// Sync-related state that is shared across all windows
pub struct SyncState {
    /// Sync server connection status
    pub connection_status: Arc<Mutex<SynchronizationStatus>>,
    /// Timestamp when the connection attempt started (for delayed spinner display)
    pub connecting_since: Arc<Mutex<Option<std::time::Instant>>>,
    /// Device name from server
    pub device_name: Arc<Mutex<Option<String>>>,
    /// Pending shared files from sync server
    pub pending_shared_files: Arc<Mutex<Vec<SharedFileResponse>>>,
    /// JWT token state manager with condition variable for efficient token refresh coordination
    pub token_state: Arc<crate::fulgur::sync::access_token::TokenStateManager>,
    /// Last heartbeat time for sync connection
    pub last_heartbeat: Arc<Mutex<Option<std::time::Instant>>>,
    /// Flag to track if sync has been initialized (to prevent multiple initializations)
    pub initialized: Arc<AtomicBool>,
    /// Pending notification from background sync operations (checked in render loop)
    pub pending_notification: Arc<Mutex<Option<(NotificationType, SharedString)>>>,
    /// Pending devices list from background fetch (checked in render loop to open share sheet)
    pub pending_devices: Arc<Mutex<Option<PendingDevicesResult>>>,
    /// Maximum file size for sharing (bytes), as reported by the server.
    pub max_file_size_bytes: Arc<AtomicU64>,
}

impl SyncState {
    /// Create a new sync state with the given connection status
    ///
    /// ### Arguments
    /// - `connection_status`: The initial sync server connection status
    ///
    /// ### Returns
    /// - `Self`: The new sync state
    pub fn new(connection_status: SynchronizationStatus) -> Self {
        Self {
            connection_status: Arc::new(Mutex::new(connection_status)),
            connecting_since: Arc::new(Mutex::new(None)),
            device_name: Arc::new(Mutex::new(None)),
            pending_shared_files: Arc::new(Mutex::new(Vec::new())),
            token_state: Arc::new(crate::fulgur::sync::access_token::TokenStateManager::new()),
            last_heartbeat: Arc::new(Mutex::new(None)),
            initialized: Arc::new(AtomicBool::new(false)),
            pending_notification: Arc::new(Mutex::new(None)),
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
    /// Sync-related state (connection status, device name, pending files, token state, heartbeat, initialization flag)
    pub sync_state: SyncState,
    /// Synchronization initialization error (if key generation failed)
    pub sync_error: Arc<Mutex<Option<String>>>,
    /// Update info if available
    pub update_info: Arc<Mutex<Option<UpdateInfo>>>,
    /// Files from macOS "Open with" events (already `Arc<Mutex>`)
    pub pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>,
    /// Pending IPC commands from Windows jump list ("new-tab", "new-window")
    #[cfg(target_os = "windows")]
    pub pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    /// Shared HTTP agent for connection pooling across all requests
    pub http_agent: Arc<ureq::Agent>,
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

        Self {
            settings: Arc::new(Mutex::new(settings)),
            settings_version: Arc::new(AtomicU64::new(0)),
            themes: Arc::new(Mutex::new(themes)),
            sync_state: SyncState::new(synchronization_status),
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
        let sync_error = if settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            match check_private_public_keys(&mut settings) {
                Ok(_) => None,
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
    /// - `SynchronizationStatus`: Connecting if sync is activated, NotActivated otherwise
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
}
