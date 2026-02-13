use std::{fmt, sync::Arc, thread, time::Duration};

use fulgur_common::api::sync::{BeginResponse, InitialSynchronizationPayload};
use gpui::{App, Entity, SharedString};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;

use super::access_token::{TokenState, get_valid_token};
use super::sse::connect_sse;
use crate::fulgur::settings::SynchronizationSettings;
use crate::fulgur::utils::crypto_helper::load_device_api_key_from_keychain;

/// Create a ureq agent with configured timeouts for all sync operations
///
/// ### Description
/// ureq 3.x `Agent::new_with_defaults()` provides:
/// - **30 second connection timeout**: Prevents indefinite hanging when establishing connections
/// - **No read timeout**: Suitable for both quick API calls and long-lived SSE connections
///
/// ### Returns
/// - `ureq::Agent`: A configured HTTP agent with appropriate timeouts
pub(super) fn create_http_agent() -> ureq::Agent {
    ureq::Agent::new_with_defaults()
}

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
pub(super) fn handle_ureq_error(error: ureq::Error, context: &str) -> SynchronizationError {
    match error {
        ureq::Error::StatusCode(code) => {
            log::error!("{}: HTTP status {}", context, code);
            if code == 401 || code == 403 {
                SynchronizationError::AuthenticationFailed
            } else if code == 400 {
                SynchronizationError::BadRequest
            } else {
                SynchronizationError::ServerError(code)
            }
        }
        ureq::Error::Io(io_error) => {
            log::error!("{} (IO): {}", context, io_error);
            match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => SynchronizationError::ConnectionFailed,
                std::io::ErrorKind::TimedOut => {
                    SynchronizationError::Timeout(io_error.to_string())
                }
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted => {
                    SynchronizationError::ConnectionFailed
                }
                std::io::ErrorKind::AddrNotAvailable => SynchronizationError::HostNotFound,
                _ => SynchronizationError::Other(io_error.to_string()),
            }
        }
        ureq::Error::ConnectionFailed => {
            log::error!("{}: Connection failed", context);
            SynchronizationError::ConnectionFailed
        }
        ureq::Error::HostNotFound => {
            log::error!("{}: Host not found", context);
            SynchronizationError::HostNotFound
        }
        ureq::Error::Timeout(timeout) => {
            log::error!("{}: Timeout ({})", context, timeout);
            SynchronizationError::Timeout(timeout.to_string())
        }
        e => {
            log::error!("{}: {}", context, e);
            SynchronizationError::Other(e.to_string())
        }
    }
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
    token_state: Arc<Mutex<TokenState>>,
) -> Result<BeginResponse, SynchronizationError> {
    let Some(server_url) = synchronization_settings.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let Some(public_key) = synchronization_settings.public_key.clone() else {
        return Err(SynchronizationError::MissingEncryptionKey); //TODO
    };
    let token = get_valid_token(synchronization_settings, token_state)?;
    let begin_url = format!("{}/api/begin", server_url);
    let payload = InitialSynchronizationPayload {
        public_key: public_key,
    };
    let agent = create_http_agent();
    let mut response = agent
        .post(begin_url)
        .header("Authorization", &format!("Bearer {}", token))
        .send_json(payload)
        .map_err(|e| handle_ureq_error(e, "Failed to begin synchronization"))?;
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read response body: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let begin_response: BeginResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse response body: {}", e);
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
        .sync_initialized
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        log::debug!("Sync already initialized by another window");
        return;
    }
    log::info!("Initializing sync system");
    let settings = entity.read(cx).settings.clone();
    let sync_server_connection_status = shared.sync_server_connection_status.clone();
    let pending_shared_files = shared.pending_shared_files.clone();
    let device_name = shared.device_name.clone();
    let sse_tx = entity.read(cx).sse_state.sse_event_tx.clone();
    let sse_shutdown_flag = entity.read(cx).sse_state.sse_shutdown_flag.clone();
    let token_state = shared.token_state.clone();
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
            Err(_) => {
                set_sync_server_connection_status(
                    sync_server_connection_status.clone(),
                    SynchronizationStatus::Disconnected,
                );
                return;
            }
        };
        if server_url.is_none() || email.is_none() || key.is_none() {
            set_sync_server_connection_status(
                sync_server_connection_status.clone(),
                SynchronizationStatus::Disconnected,
            );
            return;
        }
        match initial_synchronization(
            &settings.app_settings.synchronization_settings,
            Arc::clone(&token_state),
        ) {
            Ok(begin_response) => {
                log::info!("Successfully connected to sync server");
                set_sync_server_connection_status(
                    sync_server_connection_status.clone(),
                    SynchronizationStatus::Connected,
                );
                {
                    let mut device_name = device_name.lock();
                    *device_name = Some(begin_response.device_name);
                }
                {
                    let mut files = pending_shared_files.lock();
                    *files = begin_response.shares;
                }
                if let (Some(tx), Some(shutdown)) = (sse_tx, sse_shutdown_flag) {
                    log::info!("Starting SSE connection for real-time updates");
                    if let Err(e) = connect_sse(
                        &settings.app_settings.synchronization_settings,
                        tx,
                        shutdown,
                        sync_server_connection_status.clone(),
                        Arc::clone(&token_state),
                    ) {
                        log::error!("Failed to start SSE connection: {}", e);
                    }
                } else {
                    log::warn!(
                        "SSE event sender or shutdown flag not available, cannot start SSE connection"
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to fetch shared files: {}", e);
                set_sync_server_connection_status(
                    sync_server_connection_status,
                    SynchronizationStatus::Disconnected,
                );
            }
        }
    });
}

/// Get the synchronization status of the sync server
///
/// ### Arguments
/// - `sync_server_connection_status`: The synchronization status of the sync server
///
/// ### Returns
/// - `SynchronizationStatus`: The synchronization status of the sync server
#[allow(dead_code)]
pub fn get_sync_server_connection_status(
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
) -> SynchronizationStatus {
    *sync_server_connection_status.lock()
}

/// Set the synchronization status of the sync server
///
/// ### Arguments
/// - `sync_server_connection_status`: The synchronization status of the sync server
/// - `status`: The new synchronization status
pub fn set_sync_server_connection_status(
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    new_status: SynchronizationStatus,
) {
    *sync_server_connection_status.lock() = new_status;
}

/// Perform initial synchronization with the server
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `cx`: The context
///
/// ### Returns
/// - `SynchronizationStatus`: The status of the connection to the sync server
pub fn perform_initial_synchronization(
    entity: Entity<crate::fulgur::Fulgur>,
    cx: &mut App,
) -> SynchronizationStatus {
    let synchronization_settings = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .clone();
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let token_state = Arc::clone(&shared.token_state);
    let result = initial_synchronization(&synchronization_settings, token_state);
    let (notification, sync_server_connection_status) = match result {
        Ok(begin_response) => {
            let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
            {
                let mut name = shared.device_name.lock();
                *name = Some(begin_response.device_name.clone());
            }
            {
                let mut files = shared.pending_shared_files.lock();
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
                SharedString::from(format!("Connection failed: {}", e)),
            ),
            SynchronizationStatus::from_error(&e),
        ),
    };
    entity.update(cx, |this, cx| {
        let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
        set_sync_server_connection_status(
            shared.sync_server_connection_status.clone(),
            sync_server_connection_status,
        );
        // Store notification to be displayed on next render
        this.pending_notification = Some(notification);
    });
    sync_server_connection_status
}

pub enum SynchronizationError {
    AuthenticationFailed,
    BadRequest,
    CompressionFailed,
    ConnectionFailed,
    ContentMissing,
    ContentTooLarge,
    DeviceIdsMissing,
    DeviceKeyMissing,
    EmailMissing,
    EncryptionFailed,
    FileNameMissing,
    HostNotFound,
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
            SynchronizationError::ContentTooLarge => write!(f, "Content is too large to share"),
            SynchronizationError::DeviceIdsMissing => write!(f, "Device IDs are missing"),
            SynchronizationError::DeviceKeyMissing => write!(f, "Key is missing"),
            SynchronizationError::EmailMissing => write!(f, "Email is missing"),
            SynchronizationError::EncryptionFailed => write!(f, "Encryption failed"),
            SynchronizationError::FileNameMissing => write!(f, "File name is missing"),
            SynchronizationError::HostNotFound => write!(f, "Host not found"),
            SynchronizationError::InvalidResponse(e) => write!(f, "{}", e),
            SynchronizationError::MissingEncryptionKey => write!(f, "Missing encryption key"),
            SynchronizationError::MissingExpirationDate => write!(f, "Missing expiration date"),
            SynchronizationError::MissingPublicKey(e) => {
                write!(f, "Missing public key for device: {}", e)
            }
            SynchronizationError::Other(e) => write!(f, "{}", e),
            SynchronizationError::ServerError(e) => write!(f, "{}", e),
            SynchronizationError::ServerUrlMissing => write!(f, "Server URL is missing"),
            SynchronizationError::Timeout(timeout) => write!(f, "Timeout: {}", timeout),
        }
    }
}

#[derive(Clone, Copy)]
pub enum SynchronizationStatus {
    Connected,
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
            SynchronizationStatus::Disconnected => false,
            SynchronizationStatus::AuthenticationFailed => false,
            SynchronizationStatus::ConnectionFailed => false,
            SynchronizationStatus::Other => false,
            SynchronizationStatus::NotActivated => false,
        }
    }
}
