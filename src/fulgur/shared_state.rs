use crate::fulgur::settings::ProfileId;
use crate::fulgur::state::{RemoteHistory, StateDb, StateWriter, WindowState, WindowsState};
use crate::fulgur::sync::sse::SseState;
use crate::fulgur::sync::ssh::credentials::SshCredentialCache;
use crate::fulgur::sync::ssh::pool::SshSessionPool;
use crate::fulgur::sync::ssh::save_queue::RemoteSaveQueue;
use crate::fulgur::utils::crypto_helper::{ProfileKeyFailure, check_private_public_keys};
use crate::fulgur::utils::updater::UpdateInfo;
use crate::fulgur::{
    settings::Settings, settings::SettingsWriter, settings::Themes,
    sync::synchronization::SynchronizationStatus,
};
use fulgur_common::api::shares::SharedFileResponse;
use futures::StreamExt;
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::{Notification, NotificationDelivery, NotificationType};
use gpui_kit::{App, AsyncApp, SharedString};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};
use std::time::Duration;

/// Where a notification came from, which decides whether it is worth surfacing outside the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationOrigin {
    /// The user triggered the action and is looking at the window, so the in-app toast suffices.
    #[default]
    Foreground,
    /// Background work the user did not just ask for: sync, shares, SSE, startup restore.
    Background,
}

/// A user-facing notification: severity, message, and where it came from.
#[derive(Debug, Clone)]
pub struct AppNotification {
    pub notification_type: NotificationType,
    pub message: SharedString,
    pub origin: NotificationOrigin,
}

impl AppNotification {
    /// Build a notification for work the user just triggered, shown as an in-app toast only.
    ///
    /// ### Arguments
    /// - `notification_type`: The severity of the notification.
    /// - `message`: The text to display.
    ///
    /// ### Returns
    /// - `AppNotification`: The notification, tagged [`NotificationOrigin::Foreground`]
    pub fn foreground(
        notification_type: NotificationType,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            notification_type,
            message: message.into(),
            origin: NotificationOrigin::Foreground,
        }
    }

    /// Build a notification for background work, eligible for the OS notification center.
    ///
    /// ### Arguments
    /// - `notification_type`: The severity of the notification.
    /// - `message`: The text to display.
    ///
    /// ### Returns
    /// - `AppNotification`: The notification, tagged [`NotificationOrigin::Background`]
    pub fn background(
        notification_type: NotificationType,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            notification_type,
            message: message.into(),
            origin: NotificationOrigin::Background,
        }
    }
}

/// Result of a background device fetch: either a list of devices or an error message,
/// paired with a boolean indicating whether SSE reconnection is needed.
pub type PendingDevicesResult = (
    Result<Vec<fulgur_common::api::devices::DeviceResponse>, String>,
    bool,
);

/// Outcome of the encryption key check run when the app starts.
#[derive(Debug, Default)]
struct StartupKeyCheck {
    /// Active profiles whose keypair could not be ensured.
    failed_profiles: HashSet<ProfileId>,
    /// User-facing description of what failed, `None` when every check passed.
    error: Option<String>,
}

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
    /// Share IDs claimed for the v2 read/ack flow (`GET /api/v2/shares/:id`) that still need acknowledging
    /// (`POST /api/v2/shares/:id/successful`) once decryption succeeds. An id is claimed before its
    /// download and released only by a transient download failure or a successful acknowledgement,
    /// so shares whose acknowledgement failed, quarantined shares, and shares rejected as too large
    /// or malformed stay claimed and are never fetched again (see `share::fetch_unclaimed_share`).
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
    /// Serialized writer of the settings file. Every settings write goes through it.
    pub settings_writer: SettingsWriter,
    /// Available themes. Mutate via `cx.update_global` only.
    pub themes: Option<Themes>,
    /// Per-profile sync states keyed by profile id.
    pub sync_states: Arc<RwLock<HashMap<ProfileId, Arc<SyncState>>>>,
    /// Global flag to ensure sync bootstrap runs only once across all windows.
    pub sync_initialized: Arc<AtomicBool>,
    /// Synchronization initialization error (if key generation failed)
    pub sync_error: Arc<Mutex<Option<String>>>,
    /// Active profiles whose encryption keys could not be ensured at startup.
    pub startup_key_failures: HashSet<ProfileId>,
    /// Newer release found by the last successful update check. Mutate via `cx.update_global` only.
    pub available_update: Option<UpdateInfo>,
    /// Files to open from outside the app: macOS open events, command-line arguments
    /// and files forwarded by another instance.
    pub pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>,
    /// Pending IPC commands forwarded by another instance (Windows jump list) ("new-tab", "new-window")
    #[cfg(any(target_os = "windows", target_os = "linux"))]
    pub pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    /// Drop-owned handle to the single-instance IPC listener thread, set once
    /// at startup by `main.rs`.
    #[cfg(any(target_os = "windows", target_os = "linux"))]
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
    /// Process-wide queue that commits remote saves in request order per destination.
    pub remote_save_queue: Arc<RemoteSaveQueue>,
    /// Dedicated background writer for `WindowsState` persistence.
    pub state_writer: Arc<StateWriter>,
    /// URLs recently opened through the Open Remote File dialog. Mutate via
    /// `cx.update_global` only, then hand the result to `state_writer`.
    pub remote_history: RemoteHistory,
    /// Per-window restore payloads taken once at startup, indexed by window
    /// position, used to restore every window without re-reading the state
    /// database per window.
    ///
    /// Each slot is consumed by `load_state` when its window restores, so the
    /// persisted buffers live on only inside the editors that took them. Slots
    /// are never compacted: a window whose creation failed keeps its payload in
    /// place, and later windows keep their positional index.
    ///
    /// Snapshotting at startup also closes the consistency window: a
    /// `save_state` landing between a window's spawn and its restore can no
    /// longer change what that window sees.
    pub restore_state: Arc<Mutex<Vec<Option<WindowState>>>>,
    /// Sender for user-facing notifications produced anywhere in the app.
    pub notification_tx: UnboundedSender<AppNotification>,
    /// Receiver side of the notification channel, taken exactly once by `spawn_notification_consumer`.
    notification_rx: Mutex<Option<UnboundedReceiver<AppNotification>>>,
}

impl gpui_kit::Global for SharedAppState {}

impl SharedAppState {
    /// Create a new shared app state by orchestrating the initialization of shared application state
    /// by loading themes, and determining the initial synchronization status.
    ///
    /// ### Arguments
    /// - `settings`: Already-loaded application settings
    /// - `pending_files_from_macos`: Queue of files to open from outside the app, seeded with the command-line files
    /// - `restore_state`: Startup snapshot of `WindowsState`, split into one restore payload per window
    /// - `state_db`: The open session-state database, read for the remote URL
    ///   history, then handed to the writer thread which owns it for the lifetime
    ///   of the process. `None` when none could be opened, in which case session
    ///   state is not persisted.
    ///
    /// ### Returns
    /// - `Self`: The new shared app state
    pub fn new(
        mut settings: Settings,
        pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>,
        restore_state: Option<WindowsState>,
        state_db: Option<StateDb>,
    ) -> Self {
        let key_check = Self::check_startup_keys(&mut settings, check_private_public_keys);
        let themes = Self::load_themes();
        let synchronization_status = Self::determine_initial_sync_status(&settings);
        let (notification_tx, notification_rx) = unbounded();
        let sync_states = Self::seed_sync_states(
            &settings,
            synchronization_status,
            &key_check.failed_profiles,
            &notification_tx,
        );
        let remote_history = Self::load_remote_history(state_db.as_ref());

        Self {
            settings,
            settings_writer: SettingsWriter::default(),
            themes,
            sync_states: Arc::new(RwLock::new(sync_states)),
            sync_initialized: Arc::new(AtomicBool::new(false)),
            sync_error: Arc::new(Mutex::new(key_check.error)),
            startup_key_failures: key_check.failed_profiles,
            available_update: None,
            pending_files_from_macos,
            #[cfg(any(target_os = "windows", target_os = "linux"))]
            pending_ipc_commands: Arc::new(Mutex::new(Vec::new())),
            #[cfg(any(target_os = "windows", target_os = "linux"))]
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
            remote_save_queue: Arc::new(RemoteSaveQueue::new()),
            state_writer: Arc::new(StateWriter::new(state_db)),
            remote_history,
            restore_state: Arc::new(Mutex::new(
                restore_state
                    .map(|state| state.windows.into_iter().map(Some).collect())
                    .unwrap_or_default(),
            )),
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

    /// Read the remote URL history persisted in the session-state database.
    ///
    /// ### Arguments
    /// - `state_db`: The session-state database, `None` when none could be opened
    ///
    /// ### Returns
    /// - `RemoteHistory`: The persisted history, empty when there is no database
    ///   or when it cannot be read
    fn load_remote_history(state_db: Option<&StateDb>) -> RemoteHistory {
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
    fn check_startup_keys(
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
    /// - `key_failures`: Active profiles whose encryption keys could not be
    ///   ensured, seeded as disconnected instead of `default_status`.
    /// - `notification_tx`: The app-wide notification sender cloned into each state.
    ///
    /// ### Returns
    /// - `HashMap<ProfileId, Arc<SyncState>>`: Map keyed by profile id.
    fn seed_sync_states(
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
        notification.message
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
    let delivery = notification_delivery(&notification, cx);
    handle
        .update(cx, |_, window, cx| {
            window.push_notification(
                Notification::new()
                    .message(notification.message)
                    .with_type(notification.notification_type)
                    .delivery(delivery),
                cx,
            );
        })
        .is_ok()
}

/// Decide where a queued notification is presented.
///
/// ### Arguments
/// - `notification`: The notification about to be delivered.
/// - `cx`: The application context, used to read the current setting.
///
/// ### Returns
/// - `NotificationDelivery::InAppAndSystem`: The notification is background work and the user has
///   left system notifications enabled
/// - `NotificationDelivery::InApp`: Every other case
fn notification_delivery(notification: &AppNotification, cx: &App) -> NotificationDelivery {
    let system_enabled = cx
        .try_global::<SharedAppState>()
        .is_some_and(|shared| shared.settings.app_settings.system_notifications);
    if system_enabled && notification.origin == NotificationOrigin::Background {
        NotificationDelivery::InAppAndSystem
    } else {
        NotificationDelivery::InApp
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::*;
    use crate::fulgur::settings::ServerProfile;
    use crate::fulgur::state::StateDb;
    use gpui_kit::TestAppContext;

    /// Install a `SharedAppState` whose `system_notifications` setting has the given value.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `system_notifications`: The value to store in the application settings.
    fn set_up_shared_state(cx: &mut TestAppContext, system_notifications: bool) {
        cx.update(|cx| {
            let mut settings = Settings::new();
            settings.app_settings.system_notifications = system_notifications;
            let state_db = StateDb::open_in_memory().expect("failed to open state database");
            cx.set_global(SharedAppState::new(
                settings,
                Arc::new(Mutex::new(Vec::new())),
                None,
                Some(state_db),
            ));
        });
    }

    #[gpui_kit::test]
    fn test_background_notification_reaches_system_when_enabled(cx: &mut TestAppContext) {
        set_up_shared_state(cx, true);
        let notification = AppNotification::background(NotificationType::Info, "share received");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(delivery, NotificationDelivery::InAppAndSystem);
    }

    #[gpui_kit::test]
    fn test_background_notification_stays_in_app_when_disabled(cx: &mut TestAppContext) {
        set_up_shared_state(cx, false);
        let notification = AppNotification::background(NotificationType::Info, "share received");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(delivery, NotificationDelivery::InApp);
    }

    #[gpui_kit::test]
    fn test_foreground_notification_never_reaches_system(cx: &mut TestAppContext) {
        set_up_shared_state(cx, true);
        let notification =
            AppNotification::foreground(NotificationType::Error, "failed to save settings");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(
            delivery,
            NotificationDelivery::InApp,
            "a notification the user just triggered is already on screen"
        );
    }

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

    #[gpui_kit::test]
    fn test_delivery_without_shared_state_stays_in_app(cx: &mut TestAppContext) {
        let notification = AppNotification::background(NotificationType::Info, "early failure");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(delivery, NotificationDelivery::InApp);
    }
}
