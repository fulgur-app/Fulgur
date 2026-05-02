use super::access_token::{TokenStateManager, get_valid_token};
use super::sse::connect_sse;
use crate::fulgur::Fulgur;
use crate::fulgur::settings::SynchronizationSettings;
use crate::fulgur::sync::share;
use crate::fulgur::ui::tabs::editor_tab;
use crate::fulgur::ui::tabs::tab::Tab;
use crate::fulgur::utils::crypto_helper::{
    self, load_device_api_key_from_keychain, load_private_key_from_keychain,
};
use crate::fulgur::utils::sanitize::sanitize_filename;
use fulgur_common::api::sync::{BeginResponse, InitialSynchronizationPayload};
use gpui::{App, Context, Entity, SharedString, Window};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::fulgur::ui::notifications::progress::{CancelCallback, start_progress};

/// Handle ureq errors and convert them to SynchronizationError with appropriate logging
///
/// ### Description
/// Centralizes ureq error handling logic that was duplicated across sync modules.
/// Maps all ureq error variants to appropriate SynchronizationError types and logs
/// the error with context.
///
/// ### Arguments
/// - `error`: The ureq error to handle
/// - `context`: Human-readable context for logging (e.g., "Failed to get devices")
///
/// ### Returns
/// - `SynchronizationError`: The mapped synchronization error
pub fn handle_ureq_error(error: ureq::Error, context: &str) -> SynchronizationError {
    match error {
        ureq::Error::StatusCode(code) => {
            log::error!("{context}: HTTP status {code}");
            if code == 401 || code == 403 {
                SynchronizationError::AuthenticationFailed
            } else if code == 400 {
                SynchronizationError::BadRequest
            } else if code == 413 {
                SynchronizationError::ContentTooLarge {
                    file_size: 0,
                    max_size: 0,
                }
            } else {
                SynchronizationError::ServerError(code)
            }
        }
        ureq::Error::Io(io_error) => {
            log::error!("{context} (IO): {io_error}");
            match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => SynchronizationError::ConnectionFailed,
                std::io::ErrorKind::TimedOut => SynchronizationError::Timeout(io_error.to_string()),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted => {
                    SynchronizationError::ConnectionFailed
                }
                std::io::ErrorKind::AddrNotAvailable => SynchronizationError::HostNotFound,
                _ => SynchronizationError::Other(io_error.to_string()),
            }
        }
        ureq::Error::ConnectionFailed => {
            log::error!("{context}: Connection failed");
            SynchronizationError::ConnectionFailed
        }
        ureq::Error::HostNotFound => {
            log::error!("{context}: Host not found");
            SynchronizationError::HostNotFound
        }
        ureq::Error::Timeout(timeout) => {
            log::error!("{context}: Timeout ({timeout})");
            SynchronizationError::Timeout(timeout.to_string())
        }
        e => {
            log::error!("{context}: {e}");
            SynchronizationError::Other(e.to_string())
        }
    }
}

/// Validate and persist the server-advertised `max_file_size_bytes`.
///
/// ### Arguments
/// - `atomic`: The shared atomic holding the current cap
/// - `advertised`: The `Option<u64>` received from the server's response
pub fn store_server_max_file_size(atomic: &std::sync::atomic::AtomicU64, advertised: Option<u64>) {
    let value = match advertised {
        None => {
            log::info!("Server max file size: no limit");
            u64::MAX
        }
        Some(0) => {
            log::warn!(
                "Server advertised max_file_size_bytes = 0 (would disable sharing); falling back to {} bytes",
                share::MAX_SYNC_SHARE_PAYLOAD_BYTES
            );
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        }
        Some(n) => {
            log::info!("Server max file size: {n} bytes");
            n
        }
    };
    atomic.store(value, std::sync::atomic::Ordering::Release);
}

/// Initial synchronization with the server. This endpoint returns both the encryption key and any shared files waiting for this device.
///
/// ### Arguments
/// - `server_url`: The server URL
/// - `email`: The email
/// - `key`: The encrypted device authentication key
///
/// ### Returns
/// - `Ok(BeginResponse)`: The begin response containing encryption key and shared files
/// - `Err(SynchronizationError)`: If the synchronization could not be performed
pub fn initial_synchronization(
    synchronization_settings: &SynchronizationSettings,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<BeginResponse, SynchronizationError> {
    let Some(server_url) = synchronization_settings.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let Some(public_key) = synchronization_settings.public_key.clone() else {
        return Err(SynchronizationError::MissingEncryptionKey); //TODO
    };
    let token = get_valid_token(synchronization_settings, token_state, http_agent)?;
    let begin_url = format!("{server_url}/api/begin");
    let payload = InitialSynchronizationPayload { public_key };
    let mut response = http_agent
        .post(begin_url)
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(payload)
        .map_err(|e| handle_ureq_error(e, "Failed to begin synchronization"))?;
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read response body: {e}");
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let begin_response: BeginResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse response body: {e}");
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    log::info!(
        "Initial synchronization successful with {} shared files",
        begin_response.shares.len()
    );
    Ok(begin_response)
}

/// Fetches shared files from the server and stores them for processing without blocking app startup
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `cx`: The application context
pub fn begin_synchronization(entity: &gpui::Entity<crate::fulgur::Fulgur>, cx: &gpui::App) {
    if !entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .is_synchronization_activated
    {
        return;
    }
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    if shared
        .sync_state
        .initialized
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        log::debug!("Sync already initialized by another window");
        return;
    }
    log::info!("Initializing sync system");
    let settings = entity.read(cx).settings.clone();
    let sync_server_connection_status = shared.sync_state.connection_status.clone();
    let pending_shared_files = shared.sync_state.pending_shared_files.clone();
    let device_name = shared.sync_state.device_name.clone();
    let sse_tx = entity.read(cx).sse_state.sse_event_tx.clone();
    let sse_shutdown_flag = entity.read(cx).sse_state.sse_shutdown_flag.clone();
    let sse_thread_handle = entity.read(cx).sse_state.sse_thread_handle.clone();
    let token_state = shared.sync_state.token_state.clone();
    let http_agent = Arc::clone(&shared.http_agent);
    let max_file_size_bytes = shared.sync_state.max_file_size_bytes.clone();
    thread::spawn(move || {
        // Small delay to ensure app initialization doesn't block
        thread::sleep(Duration::from_millis(100));
        let server_url = settings
            .app_settings
            .synchronization_settings
            .server_url
            .clone();
        let email = settings.app_settings.synchronization_settings.email.clone();
        let key = match load_device_api_key_from_keychain() {
            Ok(value) => value,
            Err(e) => {
                log::error!("Failed to load device API key from keychain: {e}");
                set_sync_server_connection_status(
                    &sync_server_connection_status,
                    SynchronizationStatus::Disconnected,
                );
                return;
            }
        };
        if server_url.is_none() || email.is_none() || key.is_none() {
            set_sync_server_connection_status(
                &sync_server_connection_status,
                SynchronizationStatus::Disconnected,
            );
            return;
        }
        match initial_synchronization(
            &settings.app_settings.synchronization_settings,
            &token_state,
            &http_agent,
        ) {
            Ok(begin_response) => {
                log::info!("Successfully connected to sync server");
                set_sync_server_connection_status(
                    &sync_server_connection_status,
                    SynchronizationStatus::Connected,
                );
                store_server_max_file_size(
                    &max_file_size_bytes,
                    begin_response.max_file_size_bytes,
                );
                {
                    let mut device_name = device_name.lock();
                    *device_name = Some(begin_response.device_name);
                }
                {
                    let mut files = pending_shared_files.lock();
                    *files = begin_response
                        .shares
                        .into_iter()
                        .map(|mut share| {
                            share.file_name = sanitize_filename(&share.file_name);
                            share
                        })
                        .collect();
                }
                if let (Some(tx), Some(shutdown)) = (sse_tx, sse_shutdown_flag) {
                    log::info!("Starting SSE connection for real-time updates");
                    match connect_sse(
                        &settings.app_settings.synchronization_settings,
                        tx,
                        shutdown,
                        sync_server_connection_status.clone(),
                        &token_state,
                        &http_agent,
                        &pending_shared_files,
                    ) {
                        Ok(handle) => {
                            *sse_thread_handle.lock() = Some(handle);
                        }
                        Err(e) => {
                            log::error!("Failed to start SSE connection: {e}");
                        }
                    }
                } else {
                    log::warn!(
                        "SSE event sender or shutdown flag not available, cannot start SSE connection"
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to fetch shared files: {e}");
                set_sync_server_connection_status(
                    &sync_server_connection_status,
                    SynchronizationStatus::Disconnected,
                );
            }
        }
    });
}

/// Set the synchronization status of the sync server
///
/// ### Arguments
/// - `sync_server_connection_status`: The synchronization status of the sync server
/// - `status`: The new synchronization status
pub fn set_sync_server_connection_status(
    sync_server_connection_status: &Arc<Mutex<SynchronizationStatus>>,
    new_status: SynchronizationStatus,
) {
    *sync_server_connection_status.lock() = new_status;
}

/// Perform initial synchronization with the server in a background thread
///
/// Sets the connection status to `Connecting` immediately, then spawns a background
/// thread to perform the actual network call. The UI remains responsive while the
/// connection is in progress.
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `cx`: The context
pub fn perform_initial_synchronization(entity: &Entity<crate::fulgur::Fulgur>, cx: &mut App) {
    let synchronization_settings = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .clone();
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    set_sync_server_connection_status(
        &shared.sync_state.connection_status,
        SynchronizationStatus::Connecting,
    );
    *shared.sync_state.connecting_since.lock() = Some(Instant::now());
    let token_state = Arc::clone(&shared.sync_state.token_state);
    let http_agent = Arc::clone(&shared.http_agent);
    let connection_status = shared.sync_state.connection_status.clone();
    let connecting_since = shared.sync_state.connecting_since.clone();
    let device_name = shared.sync_state.device_name.clone();
    let pending_shared_files = shared.sync_state.pending_shared_files.clone();
    let pending_notification = shared.sync_state.pending_notification.clone();
    let max_file_size_bytes = shared.sync_state.max_file_size_bytes.clone();
    thread::spawn(move || {
        let result = initial_synchronization(&synchronization_settings, &token_state, &http_agent);
        let (notification, status) = match result {
            Ok(begin_response) => {
                store_server_max_file_size(
                    &max_file_size_bytes,
                    begin_response.max_file_size_bytes,
                );
                {
                    let mut name = device_name.lock();
                    *name = Some(begin_response.device_name.clone());
                }
                {
                    let mut files = pending_shared_files.lock();
                    *files = begin_response.shares;
                }
                (
                    (
                        NotificationType::Success,
                        SharedString::from(format!(
                            "Connection successful as {}",
                            begin_response.device_name
                        )),
                    ),
                    SynchronizationStatus::Connected,
                )
            }
            Err(e) => (
                (
                    NotificationType::Error,
                    SharedString::from(format!("Connection failed: {e}")),
                ),
                SynchronizationStatus::from_error(&e),
            ),
        };
        set_sync_server_connection_status(&connection_status, status);
        *connecting_since.lock() = None;
        *pending_notification.lock() = Some(notification);
    });
}

/// Perform initial synchronization with a progress spinner notification.
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `window`: Target window for the notification
/// - `cx`: The application context
pub fn perform_initial_synchronization_with_progress(
    entity: &Entity<crate::fulgur::Fulgur>,
    window: &mut Window,
    cx: &mut App,
) {
    let synchronization_settings = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .clone();
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    set_sync_server_connection_status(
        &shared.sync_state.connection_status,
        SynchronizationStatus::Connecting,
    );
    *shared.sync_state.connecting_since.lock() = Some(Instant::now());
    let token_state = Arc::clone(&shared.sync_state.token_state);
    let http_agent = Arc::clone(&shared.http_agent);
    let connection_status = shared.sync_state.connection_status.clone();
    let connecting_since = shared.sync_state.connecting_since.clone();
    let device_name = shared.sync_state.device_name.clone();
    let pending_shared_files = shared.sync_state.pending_shared_files.clone();
    let pending_notification = shared.sync_state.pending_notification.clone();
    let max_file_size_bytes = shared.sync_state.max_file_size_bytes.clone();

    let done = Arc::new(AtomicBool::new(false));
    let done_for_thread = Arc::clone(&done);

    let cancel_status = connection_status.clone();
    let cancel_connecting_since = connecting_since.clone();
    let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, _cx| {
        set_sync_server_connection_status(&cancel_status, SynchronizationStatus::Disconnected);
        *cancel_connecting_since.lock() = None;
    }));

    let progress = start_progress(
        window,
        cx,
        "Connecting to Fulgurant...".into(),
        cancel_callback,
    );
    let cancel_flag = progress.cancel_flag();
    let cancel_flag_for_thread = Arc::clone(&cancel_flag);

    thread::spawn(move || {
        let result = initial_synchronization(&synchronization_settings, &token_state, &http_agent);

        if cancel_flag_for_thread.load(Ordering::Acquire) {
            done_for_thread.store(true, Ordering::Release);
            return;
        }

        let (notification, status) = match result {
            Ok(begin_response) => {
                store_server_max_file_size(
                    &max_file_size_bytes,
                    begin_response.max_file_size_bytes,
                );
                {
                    let mut name = device_name.lock();
                    *name = Some(begin_response.device_name.clone());
                }
                {
                    let mut files = pending_shared_files.lock();
                    *files = begin_response.shares;
                }
                (
                    (
                        NotificationType::Success,
                        SharedString::from(format!(
                            "Connection successful as {}",
                            begin_response.device_name
                        )),
                    ),
                    SynchronizationStatus::Connected,
                )
            }
            Err(e) => (
                (
                    NotificationType::Error,
                    SharedString::from(format!("Connection failed: {e}")),
                ),
                SynchronizationStatus::from_error(&e),
            ),
        };
        set_sync_server_connection_status(&connection_status, status);
        *connecting_since.lock() = None;
        *pending_notification.lock() = Some(notification);
        done_for_thread.store(true, Ordering::Release);
    });

    window
        .spawn(cx, async move |async_cx| {
            let _progress = progress;
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if done.load(Ordering::Acquire) || cancel_flag.load(Ordering::Acquire) {
                    break;
                }
            }
        })
        .detach();
}

#[derive(Debug)]
pub enum SynchronizationError {
    AuthenticationFailed,
    BadRequest,
    CompressionFailed,
    ConnectionFailed,
    ContentMissing,
    ContentTooLarge { file_size: usize, max_size: usize },
    DeviceIdsMissing,
    DeviceKeyMissing,
    EmailMissing,
    EncryptionFailed,
    FileNameMissing,
    HostNotFound,
    InvalidPublicKey(String),
    InvalidResponse(String),
    MissingEncryptionKey,
    MissingPublicKey(String),
    MissingExpirationDate,
    Other(String),
    ServerError(u16),
    ServerUrlMissing,
    Timeout(String),
}

impl fmt::Display for SynchronizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynchronizationError::AuthenticationFailed => write!(f, "Authentication failed"),
            SynchronizationError::BadRequest => write!(f, "Bad request"),
            SynchronizationError::CompressionFailed => write!(f, "Compression failed"),
            SynchronizationError::ConnectionFailed => write!(f, "Cannot connect to sync server"),
            SynchronizationError::ContentMissing => write!(f, "Content is missing"),
            SynchronizationError::ContentTooLarge {
                file_size: 0,
                max_size: 0,
            } => write!(f, "File is too large to share (rejected by server)"),
            SynchronizationError::ContentTooLarge {
                file_size,
                max_size,
            } => write!(
                f,
                "File is too large to share ({} KB, max {} KB)",
                file_size / 1024,
                max_size / 1024
            ),
            SynchronizationError::DeviceIdsMissing => write!(f, "Device IDs are missing"),
            SynchronizationError::DeviceKeyMissing => write!(f, "Key is missing"),
            SynchronizationError::EmailMissing => write!(f, "Email is missing"),
            SynchronizationError::EncryptionFailed => write!(f, "Encryption failed"),
            SynchronizationError::FileNameMissing => write!(f, "File name is missing"),
            SynchronizationError::HostNotFound => write!(f, "Host not found"),
            SynchronizationError::InvalidPublicKey(name) => {
                write!(f, "Invalid public key for device: {name}")
            }
            SynchronizationError::InvalidResponse(e) => write!(f, "{e}"),
            SynchronizationError::MissingEncryptionKey => write!(f, "Missing encryption key"),
            SynchronizationError::MissingExpirationDate => write!(f, "Missing expiration date"),
            SynchronizationError::MissingPublicKey(e) => {
                write!(f, "Missing public key for device: {e}")
            }
            SynchronizationError::Other(e) => write!(f, "{e}"),
            SynchronizationError::ServerError(e) => write!(f, "{e}"),
            SynchronizationError::ServerUrlMissing => write!(f, "Server URL is missing"),
            SynchronizationError::Timeout(timeout) => write!(f, "Timeout: {timeout}"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum SynchronizationStatus {
    Connected,
    Connecting,
    Disconnected,
    AuthenticationFailed,
    ConnectionFailed,
    Other,
    NotActivated,
}

impl SynchronizationStatus {
    /// Convert the error to a synchronization status
    ///
    /// ### Arguments
    /// - `error`: The error
    ///
    /// ### Returns
    /// - `SynchronizationStatus`: The synchronization status
    pub fn from_error(error: &SynchronizationError) -> SynchronizationStatus {
        match error {
            SynchronizationError::AuthenticationFailed => {
                SynchronizationStatus::AuthenticationFailed
            }
            SynchronizationError::HostNotFound => SynchronizationStatus::ConnectionFailed,
            SynchronizationError::ConnectionFailed => SynchronizationStatus::ConnectionFailed,
            SynchronizationError::Timeout(_) => SynchronizationStatus::ConnectionFailed,
            _ => SynchronizationStatus::Other,
        }
    }

    /// Check if the synchronization status is connected
    ///
    /// ### Returns
    /// - `true` if the synchronization status is connected, `false` otherwise
    pub fn is_connected(&self) -> bool {
        match self {
            SynchronizationStatus::Connected => true,
            SynchronizationStatus::Connecting => false,
            SynchronizationStatus::Disconnected => false,
            SynchronizationStatus::AuthenticationFailed => false,
            SynchronizationStatus::ConnectionFailed => false,
            SynchronizationStatus::Other => false,
            SynchronizationStatus::NotActivated => false,
        }
    }

    /// Check if the synchronization status is connecting
    ///
    /// ### Returns
    /// - `true` if the synchronization status is connecting, `false` otherwise
    pub fn is_connecting(&self) -> bool {
        matches!(self, SynchronizationStatus::Connecting)
    }
}

impl Fulgur {
    /// Process shared files from the sync server
    ///
    /// ### Arguments
    /// - `window`: The window to create new tabs in
    /// - `cx`: The application context
    pub fn process_shared_files_from_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let shared_files_to_open = if let Some(mut pending) = self
            .shared_state(cx)
            .sync_state
            .pending_shared_files
            .try_lock()
        {
            if pending.is_empty() {
                Vec::new()
            } else {
                log::info!(
                    "Processing {} shared file(s) from sync server",
                    pending.len()
                );
                pending.drain(..).collect()
            }
        } else {
            Vec::new()
        };
        if !shared_files_to_open.is_empty() {
            let encryption_key_opt = match load_private_key_from_keychain() {
                Ok(key) => key,
                Err(_) => {
                    log::error!("Cannot decrypt shared files: encryption key not available");
                    None
                }
            };
            let server_max_size = self
                .shared_state(cx)
                .sync_state
                .max_file_size_bytes
                .load(std::sync::atomic::Ordering::Acquire);
            if let Some(encryption_key) = encryption_key_opt {
                for shared_file in shared_files_to_open {
                    if server_max_size != u64::MAX
                        && shared_file.content.len() as u64 > server_max_size.saturating_mul(2)
                    {
                        log::warn!(
                            "Skipping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds 2x the server max ({} bytes)",
                            shared_file.file_name,
                            shared_file.source_device_id,
                            shared_file.content.len(),
                            server_max_size
                        );
                        continue;
                    }
                    let decrypted_result =
                        crypto_helper::decrypt_bytes(&shared_file.content, &encryption_key)
                            .and_then(|compressed_bytes| {
                                share::decompress_content(&compressed_bytes, server_max_size)
                            });
                    match decrypted_result {
                        Ok(decrypted_content) => {
                            let tab_id = self.next_tab_id;
                            self.next_tab_id += 1;
                            let new_tab = Tab::Editor(editor_tab::EditorTab::from_content(
                                tab_id,
                                &decrypted_content,
                                shared_file.file_name.clone(),
                                window,
                                cx,
                                &self.settings.editor_settings,
                            ));
                            self.tabs.push(new_tab);
                            self.active_tab_index = Some(self.tabs.len() - 1);
                            self.pending_tab_scroll = Some(self.tabs.len() - 1);
                            log::info!("Opened shared file: {}", shared_file.file_name);
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to decrypt shared file {}: {}",
                                shared_file.file_name,
                                e
                            );
                        }
                    }
                }
            } else {
                log::error!("Cannot decrypt shared files: encryption key not available");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::store_server_max_file_size;
    use crate::fulgur::sync::share;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn none_is_stored_as_unlimited() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, None);
        assert_eq!(atomic.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn zero_is_replaced_with_safe_default() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(0));
        assert_eq!(
            atomic.load(Ordering::Acquire),
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        );
    }

    #[test]
    fn positive_values_are_accepted_verbatim() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(5 * 1024 * 1024));
        assert_eq!(atomic.load(Ordering::Acquire), 5 * 1024 * 1024);
    }

    #[test]
    fn very_large_values_are_trusted_as_user_choice() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(u64::MAX - 1));
        assert_eq!(atomic.load(Ordering::Acquire), u64::MAX - 1);
    }
}
