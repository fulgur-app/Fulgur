mod notifications;
mod startup;
mod sync_state;

pub use notifications::{AppNotification, NotificationOrigin, spawn_notification_consumer};
pub use sync_state::{PendingDevicesResult, SyncState};

use crate::fulgur::settings::ProfileId;
use crate::fulgur::state::{RemoteHistory, StateDb, StateWriter, WindowState, WindowsState};
use crate::fulgur::sync::ssh::credentials::SshCredentialCache;
use crate::fulgur::sync::ssh::pool::SshSessionPool;
use crate::fulgur::sync::ssh::save_queue::RemoteSaveQueue;
use crate::fulgur::utils::crypto_helper::check_private_public_keys;
use crate::fulgur::utils::updater::UpdateInfo;
use crate::fulgur::{
    settings::Settings, settings::SettingsWriter, settings::Themes,
    sync::synchronization::SynchronizationStatus,
};
use futures::channel::mpsc::{UnboundedReceiver, UnboundedSender, unbounded};
use parking_lot::{Mutex, RwLock};
use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::AtomicBool;

/// A file queued to open from outside the app.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExternalOpenFile {
    pub path: PathBuf,
    /// Whether this file must open read-only even if the session is not, as when
    /// `--read-only` forwards it to an instance that is already running.
    pub read_only: bool,
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
    pub pending_files_from_macos: Arc<Mutex<Vec<ExternalOpenFile>>>,
    /// Whether this process was started with `--read-only`: every file it opens or
    /// restores is read-only. Set once at startup and never persisted.
    pub read_only_session: bool,
    /// Pending IPC commands forwarded by another instance ("new-tab", "new-window", "focus").
    /// Only the Windows and Linux single-instance listener fills it; it stays empty on macOS.
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
        pending_files_from_macos: Arc<Mutex<Vec<ExternalOpenFile>>>,
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
            read_only_session: false,
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
                restore_state.map_or_default(|state| state.windows.into_iter().map(Some).collect()),
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
            .map_or_default(|p| p.id.clone());
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
