use crate::fulgur::settings::ProfileId;
use crate::fulgur::state::{StateWriter, WindowsState};
use crate::fulgur::sync::sse::SseState;
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
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui::{App, AsyncApp, SharedString};
use gpui_component::WindowExt;
use gpui_component::notification::NotificationType;
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

/// A user-facing notification: severity plus message.
pub type AppNotification = (NotificationType, SharedString);

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
    /// Pending shared files arrived from this profile's server (still encrypted).
    pub pending_shared_files: Arc<Mutex<Vec<SharedFileResponse>>>,
    /// Share IDs fetched via the v2 read/ack flow (`GET /api/v2/shares/:id`) that still need acknowledging
    /// (`POST /api/v2/shares/:id/successful`) once decryption succeeds.
    pub pending_ack_share_ids: Arc<Mutex<HashSet<String>>>,
    /// Shares already decrypted off the UI thread, awaiting tab creation in the render loop.
    pub pending_decrypted_files: Arc<Mutex<Vec<crate::fulgur::sync::share::DecryptedShare>>>,
    /// Per-share decryption retry bookkeeping keyed by share id (attempt count
    /// and backoff deadline). Entries are removed on success or quarantine.
    pub share_retry_state: Arc<Mutex<HashMap<String, crate::fulgur::sync::share::ShareRetryState>>>,
    /// Guard ensuring at most one background decryption worker runs per profile.
    pub decrypt_in_flight: Arc<AtomicBool>,
    /// JWT token state manager with condition variable for efficient token
    /// refresh coordination, scoped to this profile.
    pub token_state: Arc<crate::fulgur::sync::access_token::TokenStateManager>,
    /// Last heartbeat time received from this profile's SSE stream.
    pub last_heartbeat: Arc<Mutex<Option<std::time::Instant>>>,
    /// Sender for user-facing notifications produced by background sync operations,
    /// delivered by the app-scope consumer task (see `spawn_notification_consumer`).
    pub notification_tx: UnboundedSender<AppNotification>,
    /// Last emitted receive-error signature for shared-file processing (error deduplication).
    pub last_share_receive_error_signature: Arc<Mutex<Option<String>>>,
    /// Pending devices list from background fetch (checked in render loop to open share sheet).
    pub pending_devices: Arc<Mutex<Option<PendingDevicesResult>>>,
    /// Maximum file size for sharing (bytes), as reported by this profile's server.
    pub max_file_size_bytes: Arc<AtomicU64>,
    /// Raw `x-fulgurant-version` value advertised by this profile's server, if any.
    /// `None` means the server did not advertise a version (Fulgurant before 0.7.0).
    pub server_version: Arc<Mutex<Option<String>>>,
    /// Minimum Fulgur version this profile's server requires for all its features.
    /// `None` means the server did not advertise one (legacy v1 server).
    pub server_min_fulgur_version: Arc<Mutex<Option<String>>>,
    /// SSE channel and worker lifecycle state for this profile.
    pub sse: Arc<Mutex<SseState>>,
}

impl SyncState {
    /// Create a new per-profile sync state with the given connection status.
    ///
    /// ### Arguments
    /// - `connection_status`: The initial sync server connection status.
    /// - `notification_tx`: The app-wide notification sender (cloned from `SharedAppState`).
    ///
    /// ### Returns
    /// - `Self`: The new sync state.
    #[must_use]
    pub fn new(
        connection_status: SynchronizationStatus,
        notification_tx: UnboundedSender<AppNotification>,
    ) -> Self {
        Self {
            connection_status: Arc::new(Mutex::new(connection_status)),
            connecting_since: Arc::new(Mutex::new(None)),
            device_name: Arc::new(Mutex::new(None)),
            pending_shared_files: Arc::new(Mutex::new(Vec::new())),
            pending_ack_share_ids: Arc::new(Mutex::new(HashSet::new())),
            pending_decrypted_files: Arc::new(Mutex::new(Vec::new())),
            share_retry_state: Arc::new(Mutex::new(HashMap::new())),
            decrypt_in_flight: Arc::new(AtomicBool::new(false)),
            token_state: Arc::new(crate::fulgur::sync::access_token::TokenStateManager::new()),
            last_heartbeat: Arc::new(Mutex::new(None)),
            notification_tx,
            last_share_receive_error_signature: Arc::new(Mutex::new(None)),
            pending_devices: Arc::new(Mutex::new(None)),
            max_file_size_bytes: Arc::new(AtomicU64::new(u64::MAX)),
            server_version: Arc::new(Mutex::new(None)),
            server_min_fulgur_version: Arc::new(Mutex::new(None)),
            sse: Arc::new(Mutex::new(SseState::with_channel())),
        }
    }

    /// Queue a user-facing notification for delivery to the focused window.
    ///
    /// ### Arguments
    /// - `notification`: The notification type and message to deliver.
    pub fn notify(&self, notification: AppNotification) {
        if self.notification_tx.unbounded_send(notification).is_err() {
            log::warn!("Notification channel closed, dropping sync notification");
        }
    }
}

/// State that is shared across all windows. This includes settings, themes, and sync-related state.
pub struct SharedAppState {
    /// Settings (shared across all windows). Mutate via `cx.update_global` only.
    pub settings: Settings,
    /// Available themes. Mutate via `cx.update_global` only.
    pub themes: Option<Themes>,
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
    /// Drop-owned handle to the single-instance IPC listener thread, set once
    /// at startup by `main.rs`.
    #[cfg(target_os = "windows")]
    pub ipc_listener: Option<crate::fulgur::utils::worker::Worker>,
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
    /// In-memory snapshot of `WindowsState` taken once at startup, used to
    /// restore every window without re-reading `state.json` per window.
    ///
    /// Holding the snapshot read-only also closes the consistency window: a
    /// `save_state` landing between a window's spawn and its restore can no
    /// longer change what that window sees.
    pub restore_state: Arc<Mutex<Option<WindowsState>>>,
    /// Sender for user-facing notifications produced anywhere in the app.
    pub notification_tx: UnboundedSender<AppNotification>,
    /// Receiver side of the notification channel, taken exactly once by `spawn_notification_consumer`.
    notification_rx: Mutex<Option<UnboundedReceiver<AppNotification>>>,
}

impl gpui::Global for SharedAppState {}

impl SharedAppState {
    /// Create a new shared app state by orchestrating the initialization of shared application state
    /// by loading themes, and determining the initial synchronization status.
    ///
    /// ### Arguments
    /// - `settings`: Already-loaded application settings
    /// - `pending_files_from_macos`: Arc to the pending files queue from macOS open events
    /// - `restore_state`: Startup snapshot of `WindowsState` shared with every window for restoration
    ///
    /// ### Returns
    /// - `Self`: The new shared app state
    pub fn new(
        settings: Settings,
        pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>,
        restore_state: Option<WindowsState>,
    ) -> Self {
        let (settings, sync_error) = Self::validate_settings(settings);
        let themes = Self::load_themes();
        let synchronization_status = Self::determine_initial_sync_status(&settings);
        let (notification_tx, notification_rx) = unbounded();
        let sync_states =
            Self::seed_sync_states(&settings, synchronization_status, &notification_tx);

        Self {
            settings,
            themes,
            sync_states: Arc::new(RwLock::new(sync_states)),
            sync_initialized: Arc::new(AtomicBool::new(false)),
            sync_error: Arc::new(Mutex::new(sync_error)),
            update_info: Arc::new(Mutex::new(None)),
            pending_files_from_macos,
            #[cfg(target_os = "windows")]
            pending_ipc_commands: Arc::new(Mutex::new(Vec::new())),
            #[cfg(target_os = "windows")]
            ipc_listener: None,
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
            restore_state: Arc::new(Mutex::new(restore_state)),
            notification_tx,
            notification_rx: Mutex::new(Some(notification_rx)),
        }
    }

    /// Queue a user-facing notification for delivery to the focused window.
    ///
    /// ### Arguments
    /// - `notification`: The notification type and message to deliver.
    pub fn notify(&self, notification: AppNotification) {
        if self.notification_tx.unbounded_send(notification).is_err() {
            log::warn!("Notification channel closed, dropping notification");
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
    /// - `notification_tx`: The app-wide notification sender cloned into each state.
    ///
    /// ### Returns
    /// - `HashMap<ProfileId, Arc<SyncState>>`: Map keyed by profile id.
    fn seed_sync_states(
        settings: &Settings,
        default_status: SynchronizationStatus,
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
                let status = if master_on && profile.is_active {
                    default_status
                } else {
                    SynchronizationStatus::NotActivated
                };
                (
                    profile.id.clone(),
                    Arc::new(SyncState::new(status, notification_tx.clone())),
                )
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
    #[must_use]
    pub fn sync_state_for(&self, profile_id: &str) -> Arc<SyncState> {
        if let Some(existing) = self.sync_states.read().get(profile_id) {
            return Arc::clone(existing);
        }
        let mut map = self.sync_states.write();
        Arc::clone(map.entry(profile_id.to_string()).or_insert_with(|| {
            Arc::new(SyncState::new(
                SynchronizationStatus::NotActivated,
                self.notification_tx.clone(),
            ))
        }))
    }

    /// Get the `SyncState` for the first configured profile.
    ///
    /// ### Returns
    /// - `Arc<SyncState>`: The first profile's sync state, or the
    ///   empty-id-keyed fallback state when there are no profiles.
    #[must_use]
    pub fn primary_sync_state(&self) -> Arc<SyncState> {
        let primary_id = self
            .settings
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
    #[must_use]
    pub fn remove_sync_state(&self, profile_id: &str) -> Option<Arc<SyncState>> {
        self.sync_states.write().remove(profile_id)
    }
}

/// Maximum delivery attempts before a notification with no window to show it in is dropped.
const NOTIFICATION_DELIVERY_MAX_ATTEMPTS: usize = 60;

/// Delay between notification delivery attempts while no window is available.
const NOTIFICATION_DELIVERY_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Spawn the app-scope task that delivers queued notifications to the focused window.
///
/// ### Arguments
/// - `cx`: The application context.
pub fn spawn_notification_consumer(cx: &mut App) {
    let Some(mut rx) = cx.global::<SharedAppState>().notification_rx.lock().take() else {
        return;
    };
    cx.spawn(async move |cx| {
        while let Some(notification) = rx.next().await {
            deliver_notification(notification, cx).await;
        }
    })
    .detach();
}

/// Deliver one notification to the focused window, retrying while no window exists.
///
/// ### Arguments
/// - `notification`: The notification to deliver.
/// - `cx`: The async application context.
async fn deliver_notification(notification: AppNotification, cx: &mut AsyncApp) {
    for attempt in 0..NOTIFICATION_DELIVERY_MAX_ATTEMPTS {
        if attempt > 0 {
            cx.background_executor()
                .timer(NOTIFICATION_DELIVERY_RETRY_DELAY)
                .await;
        }
        if cx.update(|cx| push_notification_to_focused_window(notification.clone(), cx)) {
            return;
        }
    }
    log::warn!(
        "No window available to display notification, dropping: {}",
        notification.1
    );
}

/// Push a notification to the focused window, falling back to any open window.
///
/// ### Arguments
/// - `notification`: The notification to display.
/// - `cx`: The application context.
///
/// ### Returns
/// - `true`: The notification was pushed to a window.
/// - `false`: No window is currently available.
fn push_notification_to_focused_window(notification: AppNotification, cx: &mut App) -> bool {
    let focused_window_id = cx
        .try_global::<crate::fulgur::window_manager::WindowManager>()
        .and_then(super::window_manager::WindowManager::get_last_focused);
    let windows = cx.windows();
    let handle = focused_window_id
        .and_then(|id| windows.iter().find(|w| w.window_id() == id).copied())
        .or_else(|| windows.first().copied());
    let Some(handle) = handle else {
        return false;
    };
    handle
        .update(cx, |_, window, cx| {
            window.push_notification(notification, cx);
        })
        .is_ok()
}
