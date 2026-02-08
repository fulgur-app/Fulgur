use crate::fulgur::utils::crypto_helper::check_private_public_keys;
use crate::fulgur::utils::updater::UpdateInfo;
use crate::fulgur::{
    settings::Settings, settings::Themes, sync::synchronization::SynchronizationStatus,
};
use fulgur_common::api::shares::SharedFileResponse;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64};

/// State that is shared across all windows. This includes settings, themes, and sync-related state.
pub struct SharedAppState {
    /// Settings (shared across all windows)
    pub settings: Arc<Mutex<Settings>>,
    /// Settings version counter, incremented whenever settings change. All windows check this to detect when they need to reload settings.
    pub settings_version: Arc<AtomicU64>,
    /// Available themes
    pub themes: Arc<Mutex<Option<Themes>>>,
    /// Sync server connection status (already Arc<Mutex>)
    pub sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    /// Device name from server (already Arc<Mutex>)
    pub device_name: Arc<Mutex<Option<String>>>,
    /// Pending shared files from sync server (already Arc<Mutex>)
    pub pending_shared_files: Arc<Mutex<Vec<SharedFileResponse>>>,
    /// JWT token state (already Arc<Mutex>)
    pub token_state: Arc<Mutex<crate::fulgur::sync::access_token::TokenState>>,
    /// Last heartbeat time for sync connection (already Arc<Mutex>)
    pub last_heartbeat: Arc<Mutex<Option<std::time::Instant>>>,
    /// Update info if available
    pub update_info: Arc<Mutex<Option<UpdateInfo>>>,
    /// Files from macOS "Open with" events (already Arc<Mutex>)
    pub pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>,
    /// Flag to track if sync has been initialized (to prevent multiple initializations)
    pub sync_initialized: Arc<AtomicBool>,
}

impl gpui::Global for SharedAppState {}

impl SharedAppState {
    /// Create a new shared app state
    ///
    /// ### Arguments
    /// - `pending_files_from_macos`: Arc to the pending files queue from macOS open events
    ///
    /// ### Returns
    /// - `Self`: The new shared app state
    pub fn new(pending_files_from_macos: Arc<Mutex<Vec<PathBuf>>>) -> Self {
        let mut settings = Settings::load().unwrap_or_else(|e| {
            log::error!(
                "Failed to load settings in shared state, using defaults: {}",
                e
            );
            Settings::new()
        });
        if settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
            && let Err(e) = check_private_public_keys(&mut settings)
        {
            log::error!(
                "Cannot create public/private keys pair, sync deactivated: {}",
                e
            );
            settings
                .app_settings
                .synchronization_settings
                .is_synchronization_activated = false;
        }

        let themes = Themes::load().ok();
        let synchronization_status = if settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            SynchronizationStatus::Connected
        } else {
            SynchronizationStatus::NotActivated
        };
        Self {
            settings: Arc::new(Mutex::new(settings)),
            settings_version: Arc::new(AtomicU64::new(0)),
            themes: Arc::new(Mutex::new(themes)),
            sync_server_connection_status: Arc::new(Mutex::new(synchronization_status)),
            device_name: Arc::new(Mutex::new(None)),
            pending_shared_files: Arc::new(Mutex::new(Vec::new())),
            token_state: Arc::new(Mutex::new(
                crate::fulgur::sync::access_token::TokenState::new(),
            )),
            last_heartbeat: Arc::new(Mutex::new(None)),
            update_info: Arc::new(Mutex::new(None)),
            pending_files_from_macos,
            sync_initialized: Arc::new(AtomicBool::new(false)),
        }
    }
}
