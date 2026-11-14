use super::AppNotification;
use crate::fulgur::sync::sse::SseState;
use crate::fulgur::sync::synchronization::SynchronizationStatus;
use fulgur_common::api::shares::SharedFileResponse;
use futures::channel::mpsc::UnboundedSender;
use parking_lot::Mutex;
use std::collections::{HashMap, HashSet};
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
