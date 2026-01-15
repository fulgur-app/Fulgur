use crate::fulgur::settings::SynchronizationSettings;
use crate::fulgur::{Fulgur, files};
use crate::fulgur::{crypto_helper, ui::icons::CustomIcon};
use chrono::{DateTime, Utc};
use flate2::Compression;
use flate2::read::{GzDecoder, GzEncoder};
use fulgur_common::api::devices::DeviceResponse;
use fulgur_common::api::{AccessTokenResponse, BeginResponse};
use gpui::{App, Context, Entity, SharedString, Window};
use gpui_component::notification::NotificationType;
use gpui_component::{Icon, WindowExt};
use parking_lot::Mutex;
use serde::Serialize;
use std::io::Read;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::Sender;
use std::time::Instant;
use std::{thread, time::Duration};

pub type Device = DeviceResponse;

/// JWT access token state for thread-safe token management
///
/// ### Fields
/// - `access_token`: The current JWT access token (None if not yet obtained)
/// - `token_expires_at`: When the current token expires (None if no token)
/// - `is_refreshing_token`: Lock flag to prevent concurrent token refreshes
pub struct TokenState {
    pub access_token: Option<String>,
    pub token_expires_at: Option<DateTime<Utc>>,
    pub is_refreshing_token: bool,
}

impl TokenState {
    /// Create a new empty TokenState
    pub fn new() -> Self {
        Self {
            access_token: None,
            token_expires_at: None,
            is_refreshing_token: false,
        }
    }
}

/// Compress content using gzip compression
///
/// ### Arguments
/// - `content`: The content to compress
///
/// ### Returns
/// - `Ok(Vec<u8>)`: The compressed content as bytes
/// - `Err(anyhow::Error)`: If the content could not be compressed
fn compress_content(content: &str) -> anyhow::Result<Vec<u8>> {
    let mut encoder = GzEncoder::new(content.as_bytes(), Compression::best());
    let mut compressed = Vec::new();
    encoder.read_to_end(&mut compressed)?;
    let original_size_kb = content.len() as f64 / 1024.0;
    let compressed_size_kb = compressed.len() as f64 / 1024.0;
    let compression_ratio = (1.0 - (compressed.len() as f64 / content.len() as f64)) * 100.0;
    log::debug!(
        "Compression: {:.2} KB -> {:.2} KB ({:.1}% reduction)",
        original_size_kb,
        compressed_size_kb,
        compression_ratio
    );
    Ok(compressed)
}

/// Decompress content that was compressed with gzip
///
/// ### Arguments
/// - `compressed`: The compressed content as bytes
///
/// ### Returns
/// - `Ok(String)`: The decompressed content as string
/// - `Err(anyhow::Error)`: If the content could not be decompressed
pub fn decompress_content(compressed: &[u8]) -> anyhow::Result<String> {
    let mut decoder = GzDecoder::new(compressed);
    let mut decompressed = String::new();
    decoder.read_to_string(&mut decompressed)?;
    Ok(decompressed)
}

/// Get the icon for the device
///
/// ### Arguments
/// - `device`: The device
///
/// ### Returns
/// - `Icon`: The icon for the device
pub fn get_icon(device: &Device) -> Icon {
    match device.device_type.to_lowercase().as_str() {
        "desktop" => Icon::new(CustomIcon::Computer),
        "laptop" => Icon::new(CustomIcon::Laptop),
        "server" => Icon::new(CustomIcon::Server),
        _ => Icon::new(CustomIcon::Computer),
    }
}

/// Request a JWT access token from the server using the device key
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings containing device key
///
/// ### Returns
/// - `Ok(AccessTokenResponse)`: The JWT access token and expiration info
/// - `Err(SynchronizationError)`: If the token request failed
fn request_access_token(
    synchronization_settings: &SynchronizationSettings,
) -> Result<AccessTokenResponse, SynchronizationError> {
    let server_url = synchronization_settings.server_url.clone();
    let email = synchronization_settings.email.clone();
    let key = synchronization_settings.key.clone();
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    if email.is_none() {
        return Err(SynchronizationError::EmailMissing);
    }
    if key.is_none() {
        return Err(SynchronizationError::DeviceKeyMissing);
    }
    let decrypted_key = crypto_helper::decrypt(&key.unwrap()).unwrap();
    let token_url = format!("{}/api/token", server_url.unwrap());
    log::debug!("Requesting JWT access token from server");
    let mut response = match ureq::post(&token_url)
        .header("Authorization", &format!("Bearer {}", decrypted_key))
        .header("X-User-Email", email.unwrap())
        .send("")
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            log::error!("Failed to obtain access token: HTTP status {}", code);
            if code == 401 || code == 403 {
                return Err(SynchronizationError::AuthenticationFailed);
            } else {
                return Err(SynchronizationError::ServerError(code));
            }
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!("Failed to obtain access token (IO): {}", io_error);
            return match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::TimedOut => Err(SynchronizationError::ConnectionFailed),
                _ => Err(SynchronizationError::Other(io_error.to_string())),
            };
        }
        Err(ureq::Error::ConnectionFailed) => {
            log::error!("Failed to obtain access token: Connection failed");
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!("Failed to obtain access token: Host not found");
            return Err(SynchronizationError::HostNotFound);
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!("Failed to obtain access token: Timeout ({})", timeout);
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(e) => {
            log::error!("Failed to obtain access token: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read access token response body: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let token_response: AccessTokenResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse access token response: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    log::info!(
        "Access token obtained successfully (expires in {} seconds)",
        token_response.expires_in
    );
    Ok(token_response)
}

/// Check if the access token is still valid (with 5-minute buffer for proactive refresh)
///
/// ### Arguments
/// - `expires_at`: The token expiration time
///
/// ### Returns
/// - `true` if the token is still valid (has >5 minutes remaining)
/// - `false` if the token is expired or will expire in <5 minutes
fn is_token_valid(expires_at: &DateTime<Utc>) -> bool {
    let now = Utc::now();
    let buffer = chrono::Duration::minutes(5);
    *expires_at > now + buffer
}

/// Get a valid JWT access token, refreshing if necessary
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `token_state`: Arc to the token state (thread-safe)
///
/// ### Returns
/// - `Ok(String)`: A valid JWT access token
/// - `Err(SynchronizationError)`: If token refresh failed
pub fn get_valid_token(
    synchronization_settings: &SynchronizationSettings,
    token_state: Arc<Mutex<TokenState>>,
) -> Result<String, SynchronizationError> {
    {
        let state = token_state.lock();
        if let (Some(token_str), Some(exp_time)) = (&state.access_token, &state.token_expires_at) {
            if is_token_valid(exp_time) {
                return Ok(token_str.clone());
            }
        }
    }
    {
        let mut state = token_state.lock();
        if let (Some(token_str), Some(exp_time)) = (&state.access_token, &state.token_expires_at) {
            if is_token_valid(exp_time) && !state.is_refreshing_token {
                return Ok(token_str.clone());
            }
        }
        if state.is_refreshing_token {
            drop(state);
            thread::sleep(Duration::from_millis(100));
            let state = token_state.lock();
            if let (Some(token_str), Some(exp_time)) =
                (&state.access_token, &state.token_expires_at)
            {
                if is_token_valid(exp_time) {
                    return Ok(token_str.clone());
                }
            }
        } else {
            state.is_refreshing_token = true;
        }
    }
    log::debug!("Access token expired or missing, requesting new token");
    let token_response = request_access_token(synchronization_settings)?;
    let expires_at = DateTime::parse_from_rfc3339(&token_response.expires_at)
        .map_err(|e| {
            log::error!("Failed to parse token expiration time: {}", e);
            SynchronizationError::Other(e.to_string())
        })?
        .with_timezone(&Utc);
    let mut state = token_state.lock();
    state.access_token = Some(token_response.access_token.clone());
    state.token_expires_at = Some(expires_at);
    state.is_refreshing_token = false;
    log::debug!("Access token refreshed successfully");
    Ok(token_response.access_token)
}

/// Get the devices from the server
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `token_state`: Arc to the token state (thread-safe)
///
/// ### Returns
/// - `Ok(Vec<Device>)`: The devices
/// - `Err(SynchronizationError)`: If the devices could not be retrieved
pub fn get_devices(
    synchronization_settings: &SynchronizationSettings,
    token_state: Arc<Mutex<TokenState>>,
) -> Result<Vec<Device>, SynchronizationError> {
    let server_url = synchronization_settings.server_url.clone();
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    let token = get_valid_token(synchronization_settings, token_state)?;
    let devices_url = format!("{}/api/devices", server_url.unwrap());
    let response = ureq::get(&devices_url)
        .header("Authorization", &format!("Bearer {}", token))
        .call();
    match response {
        Ok(mut response) => {
            let devices: Vec<Device> = match response.body_mut().read_json::<Vec<Device>>() {
                Ok(devices) => devices,
                Err(e) => {
                    log::error!("Failed to read devices: {}", e);
                    return Err(SynchronizationError::InvalidResponse(e.to_string()));
                }
            };
            log::debug!("Retrieved {} devices from server", devices.len());
            Ok(devices)
        }
        Err(ureq::Error::StatusCode(code)) => {
            log::error!("Failed to get devices: HTTP status {}", code);
            if code == 401 || code == 403 {
                Err(SynchronizationError::AuthenticationFailed)
            } else {
                Err(SynchronizationError::ServerError(code))
            }
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!("Failed to get devices (IO): {}", io_error);
            match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::TimedOut => {
                    Err(SynchronizationError::Timeout(io_error.to_string()))
                }
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted => {
                    Err(SynchronizationError::Other(io_error.to_string()))
                }
                _ => Err(SynchronizationError::Other(io_error.to_string())),
            }
        }
        Err(ureq::Error::ConnectionFailed) => {
            log::error!("Failed to get devices: Connection failed");
            Err(SynchronizationError::ConnectionFailed)
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!("Failed to get devices: Host not found");
            Err(SynchronizationError::HostNotFound)
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!("Failed to get devices: Timeout ({})", timeout);
            Err(SynchronizationError::Timeout(timeout.to_string()))
        }
        Err(e) => {
            log::error!("Failed to get devices: {}", e);
            Err(SynchronizationError::Other(e.to_string()))
        }
    }
}

/// Fetch the user's encryption key from the server. The server manages a shared encryption key per user that all their devices can access.
///
/// ### Arguments
/// - `server_url`: The server URL
/// - `email`: The user's email
/// - `device_key`: The decrypted device authentication key
///
/// ### Returns
/// - `Ok(String)`: The user's encryption key (base64-encoded)
/// - `Err(SynchronizationError)`: If the encryption key could not be fetched
fn fetch_encryption_key(
    synchronization_settings: &SynchronizationSettings,
    token_state: Arc<Mutex<TokenState>>,
) -> Result<String, SynchronizationError> {
    let server_url = synchronization_settings.server_url.clone();
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    let token = get_valid_token(synchronization_settings, Arc::clone(&token_state))?;
    let key_url = format!("{}/api/encryption-key", server_url.unwrap());
    let mut response = match ureq::get(&key_url)
        .header("Authorization", &format!("Bearer {}", token))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            log::error!("Failed to fetch encryption key: HTTP status {}", code);
            if code == 401 || code == 403 {
                return Err(SynchronizationError::AuthenticationFailed);
            } else {
                return Err(SynchronizationError::ServerError(code));
            }
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!("Failed to fetch encryption key (IO): {}", io_error);
            return match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::TimedOut => Err(SynchronizationError::ConnectionFailed),
                _ => Err(SynchronizationError::Other(io_error.to_string())),
            };
        }
        Err(ureq::Error::ConnectionFailed) => {
            log::error!("Failed to fetch encryption key: Connection failed");
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!("Failed to fetch encryption key: Host not found");
            return Err(SynchronizationError::HostNotFound);
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!("Failed to fetch encryption key: Timeout ({})", timeout);
            return Err(SynchronizationError::Timeout(timeout.to_string()));
        }
        Err(e) => {
            log::error!("Failed to fetch encryption key: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let body = match response.body_mut().read_to_string() {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read response body: {}", e);
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    let json: serde_json::Value = match serde_json::from_str(&body) {
        Ok(json) => json,
        Err(e) => {
            log::error!("Failed to parse response body: {}", e);
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    let encryption_key = json["encryption_key"]
        .as_str()
        .ok_or_else(|| SynchronizationError::MissingEncryptionKey)?;
    log::debug!("Fetched encryption key from server");
    Ok(encryption_key.to_string())
}

#[derive(Serialize)]
pub struct ShareFilePayload {
    pub content: String,
    pub file_name: String,
    pub device_ids: Vec<String>,
}

/// Share the file with the devices
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `payload`: The payload to share the file with (content will be encrypted)
///
/// ### Returns
/// - `Ok(String)`: The expiration date of the shared file
/// - `Err(SynchronizationError)`: If the file could not be shared
pub fn share_file(
    synchronization_settings: &SynchronizationSettings,
    payload: ShareFilePayload,
    token_state: Arc<Mutex<TokenState>>,
) -> Result<String, SynchronizationError> {
    let server_url = synchronization_settings.server_url.clone();
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    if payload.content.is_empty() {
        return Err(SynchronizationError::ContentMissing);
    }
    if payload.content.len() > 1024 * 1024 {
        // 1MB
        return Err(SynchronizationError::ContentTooLarge);
    }
    if payload.file_name.is_empty() {
        return Err(SynchronizationError::FileNameMissing);
    }
    if payload.device_ids.is_empty() {
        return Err(SynchronizationError::DeviceIdsMissing);
    }
    let token = get_valid_token(synchronization_settings, Arc::clone(&token_state))?;
    let encryption_key = fetch_encryption_key(synchronization_settings, Arc::clone(&token_state))?;
    let server_url_str = server_url.as_ref().unwrap();
    let compressed_content = match compress_content(&payload.content) {
        Ok(content) => content,
        Err(e) => {
            log::error!("Failed to compress content: {}", e);
            return Err(SynchronizationError::CompressionFailed);
        }
    };
    let encrypted_content = match crypto_helper::encrypt_bytes(&compressed_content, &encryption_key)
    {
        Ok(content) => content,
        Err(e) => {
            log::error!("Failed to encrypt content: {}", e);
            return Err(SynchronizationError::EncryptionFailed);
        }
    };
    let encrypted_payload = ShareFilePayload {
        content: encrypted_content,
        file_name: payload.file_name.clone(),
        device_ids: payload.device_ids,
    };
    let share_url = format!("{}/api/share", server_url_str);
    let mut response = match ureq::post(&share_url)
        .header("Authorization", &format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload)
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            log::error!("Failed to share file: HTTP status {}", code);
            if code == 401 || code == 403 {
                return Err(SynchronizationError::AuthenticationFailed);
            } else if code == 400 {
                return Err(SynchronizationError::BadRequest);
            } else {
                return Err(SynchronizationError::ServerError(code));
            }
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!("Failed to share file (IO): {}", io_error);
            return match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::TimedOut => {
                    Err(SynchronizationError::Timeout(io_error.to_string()))
                }
                _ => Err(SynchronizationError::Other(io_error.to_string())),
            };
        }
        Err(ureq::Error::ConnectionFailed) => {
            log::error!("Failed to share file: Connection failed");
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!("Failed to share file: Host not found");
            return Err(SynchronizationError::HostNotFound);
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!("Failed to share file: Timeout ({})", timeout);
            return Err(SynchronizationError::Timeout(timeout.to_string()));
        }
        Err(e) => {
            log::error!("Failed to share file: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    if response.status() == 200 {
        let body = match response.body_mut().read_to_string() {
            Ok(body) => body,
            Err(e) => {
                log::error!("Failed to read response body: {}", e);
                return Err(SynchronizationError::InvalidResponse(e.to_string()));
            }
        };
        let json: serde_json::Value = match serde_json::from_str(&body) {
            Ok(json) => json,
            Err(e) => {
                log::error!("Failed to parse response body: {}", e);
                return Err(SynchronizationError::InvalidResponse(e.to_string()));
            }
        };
        let expiration_date = json["expiration_date"]
            .as_str()
            .ok_or_else(|| SynchronizationError::MissingExpirationDate)?;
        log::info!(
            "File {} shared successfully until {}",
            payload.file_name,
            expiration_date
        );
        Ok(expiration_date.to_string())
    } else {
        Err(SynchronizationError::ServerError(
            response.status().as_u16(),
        ))
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
    let server_url = synchronization_settings.server_url.clone();
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    let token = get_valid_token(synchronization_settings, token_state)?;
    let server_url_str = server_url.as_ref().unwrap();
    let begin_url = format!("{}/api/begin", server_url_str);
    let mut response = match ureq::get(&begin_url)
        .header("Authorization", &format!("Bearer {}", token))
        .call()
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            log::error!("Failed to begin synchronization: HTTP status {}", code);
            if code == 401 || code == 403 {
                return Err(SynchronizationError::AuthenticationFailed);
            } else {
                return Err(SynchronizationError::ServerError(code));
            }
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!("Failed to begin synchronization (IO): {}", io_error);
            return match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::TimedOut => {
                    Err(SynchronizationError::Timeout(io_error.to_string()))
                }
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted => {
                    Err(SynchronizationError::ConnectionFailed)
                }
                std::io::ErrorKind::AddrNotAvailable => Err(SynchronizationError::HostNotFound),
                _ => Err(SynchronizationError::Other(io_error.to_string())),
            };
        }
        Err(ureq::Error::ConnectionFailed) => {
            log::error!("Failed to begin synchronization: Connection failed");
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!("Failed to begin synchronization: Host not found");
            return Err(SynchronizationError::HostNotFound);
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!("Failed to begin synchronization: Timeout ({})", timeout);
            return Err(SynchronizationError::Timeout(timeout.to_string()));
        }
        Err(e) => {
            log::error!("Failed to begin synchronization: {}", e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
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
    let settings = entity.read(cx).settings.clone();
    let sync_server_connection_status = entity.read(cx).sync_server_connection_status.clone();
    let pending_shared_files = entity.read(cx).pending_shared_files.clone();
    let encryption_key = entity.read(cx).encryption_key.clone();
    let device_name = entity.read(cx).device_name.clone();
    let sse_tx = entity.read(cx).sse_event_tx.clone();
    let sse_shutdown_flag = entity.read(cx).sse_shutdown_flag.clone();
    let token_state = entity.read(cx).token_state.clone();
    thread::spawn(move || {
        // Small delay to ensure app initialization doesn't block
        thread::sleep(Duration::from_millis(100));
        let server_url = settings
            .app_settings
            .synchronization_settings
            .server_url
            .clone();
        let email = settings.app_settings.synchronization_settings.email.clone();
        let key = settings.app_settings.synchronization_settings.key.clone();
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
                    let mut key = encryption_key.lock();
                    *key = Some(begin_response.encryption_key);
                }
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
                        log::error!("Failed to start SSE connection: {}", e.to_string());
                    }
                } else {
                    log::warn!(
                        "SSE event sender or shutdown flag not available, cannot start SSE connection"
                    );
                }
            }
            Err(e) => {
                log::error!("Failed to fetch shared files: {}", e.to_string());
                set_sync_server_connection_status(
                    sync_server_connection_status,
                    SynchronizationStatus::Disconnected,
                );
            }
        }
    });
}

/// Connect to SSE (Server-Sent Events) endpoint on the sync serverfor real-time notifications
///
/// ### Description
/// Establishes a persistent connection to the server's SSE endpoint to receive:
/// - Heartbeat events to keep connection alive
/// - Share notifications when files are shared from other devices
/// The connection runs in a background thread and automatically reconnects on failure.
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings containing server URL, email, and key
/// - `event_tx`: Channel sender for sending SSE events to the main thread
/// - `shutdown_flag`: Atomic boolean flag to signal the SSE thread to shutdown
/// - `sync_server_connection_status`: Arc-wrapped connection status to update on connection/disconnection
///
/// ### Returns
/// - `Ok(())`: If the SSE connection thread was spawned successfully
/// - `Err(SynchronizationError)`: If required settings are missing
pub fn connect_sse(
    synchronization_settings: &SynchronizationSettings,
    event_tx: Sender<SseEvent>,
    shutdown_flag: Arc<AtomicBool>,
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    token_state: Arc<Mutex<TokenState>>,
) -> Result<(), SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .clone()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    let sse_url = format!("{}/api/sse", server_url);
    let settings_clone = synchronization_settings.clone();
    let token_state_clone = Arc::clone(&token_state);
    thread::spawn(move || {
        loop {
            if shutdown_flag.load(Ordering::Relaxed) {
                log::info!("SSE connection shutdown requested, stopping...");
                break;
            }
            let token = match get_valid_token(&settings_clone, Arc::clone(&token_state_clone)) {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Failed to get valid token for SSE: {}", e.to_string());
                    set_sync_server_connection_status(
                        sync_server_connection_status.clone(),
                        SynchronizationStatus::AuthenticationFailed,
                    );
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            log::info!("Connecting to SSE endpoint: {}", sse_url);
            let response = match ureq::get(&sse_url)
                .header("Authorization", &format!("Bearer {}", token))
                .header("Accept", "text/event-stream")
                .call()
            {
                Ok(resp) => {
                    set_sync_server_connection_status(
                        sync_server_connection_status.clone(),
                        SynchronizationStatus::Connected,
                    );
                    log::info!("SSE connection established");
                    resp
                }
                Err(e) => {
                    log::error!("SSE connection failed: {}", e);
                    set_sync_server_connection_status(
                        sync_server_connection_status.clone(),
                        SynchronizationStatus::Disconnected,
                    );
                    event_tx.send(SseEvent::Error(e.to_string())).ok();
                    if shutdown_flag.load(Ordering::Relaxed) {
                        log::info!("SSE connection shutdown requested, stopping...");
                        break;
                    }
                    thread::sleep(Duration::from_secs(5));
                    continue;
                }
            };
            let mut response = response;
            let reader = std::io::BufReader::new(response.body_mut().as_reader());
            let mut current_event_type = String::new();
            let mut current_data = String::new();
            use std::io::BufRead;
            for line in reader.lines() {
                if shutdown_flag.load(Ordering::Relaxed) {
                    log::info!(
                        "SSE connection shutdown requested during event reading, stopping..."
                    );
                    break;
                }
                match line {
                    Ok(line) => {
                        if line.starts_with("event:") {
                            current_event_type =
                                line.trim_start_matches("event:").trim().to_string();
                        } else if line.starts_with("data:") {
                            current_data.push_str(line.trim_start_matches("data:").trim());
                        } else if line.is_empty() && !current_data.is_empty() {
                            log::info!("SSE event type: {}", current_event_type);
                            log::info!("SSE data: {}", current_data);
                            let event = SseEvent::parse(&current_event_type, &current_data);
                            if let Err(e) = event_tx.send(event) {
                                log::error!("Failed to send SSE event: {}", e);
                                break;
                            }
                            current_event_type.clear();
                            current_data.clear();
                        }
                    }
                    Err(e) => {
                        log::error!("SSE stream error: {}", e);
                        set_sync_server_connection_status(
                            sync_server_connection_status.clone(),
                            SynchronizationStatus::Disconnected,
                        );
                        event_tx.send(SseEvent::Error(e.to_string())).ok();
                        break;
                    }
                }
            }
            if shutdown_flag.load(Ordering::Relaxed) {
                log::info!("SSE connection shutdown requested, stopping...");
                break;
            }
            // When the connection is lost
            log::warn!("SSE connection closed, reconnecting in 5s...");
            set_sync_server_connection_status(
                sync_server_connection_status.clone(),
                SynchronizationStatus::Disconnected,
            );
            thread::sleep(Duration::from_secs(5));
        }
    });

    Ok(())
}

/// Get the synchronization status of the sync server
///
/// ### Arguments
/// - `sync_server_connection_status`: The synchronization status of the sync server
///
/// ### Returns
/// - `SynchronizationStatus`: The synchronization status of the sync server
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
    let token_state = Arc::clone(&entity.read(cx).token_state);
    let result = initial_synchronization(&synchronization_settings, token_state);
    let (notification, sync_server_connection_status) = match result {
        Ok(begin_response) => {
            entity.update(cx, |this, _cx| {
                {
                    let mut key = this.encryption_key.lock();
                    *key = Some(begin_response.encryption_key);
                }
                {
                    let mut name = this.device_name.lock();
                    *name = Some(begin_response.device_name.clone());
                }
                {
                    let mut files = this.pending_shared_files.lock();
                    *files = begin_response.shares;
                }
            });
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
                SharedString::from(format!("Connection failed: {}", e.to_string())),
            ),
            SynchronizationStatus::from_error(&e),
        ),
    };
    entity.update(cx, |this, _cx| {
        set_sync_server_connection_status(
            this.sync_server_connection_status.clone(),
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
    MissingExpirationDate,
    Other(String),
    ServerError(u16),
    ServerUrlMissing,
    Timeout(String),
}

impl SynchronizationError {
    /// Convert the error to a string
    ///
    /// ### Returns
    /// - `String`: The error message
    pub fn to_string(&self) -> String {
        match self {
            SynchronizationError::AuthenticationFailed => "Authentication failed".to_string(),
            SynchronizationError::BadRequest => "Bad request".to_string(),
            SynchronizationError::CompressionFailed => "Compression failed".to_string(),
            SynchronizationError::ConnectionFailed => "Cannot connect to sync server".to_string(),
            SynchronizationError::ContentMissing => "Content is missing".to_string(),
            SynchronizationError::ContentTooLarge => "Content is too large to share".to_string(),
            SynchronizationError::DeviceIdsMissing => "Device IDs are missing".to_string(),
            SynchronizationError::DeviceKeyMissing => "Key is missing".to_string(),
            SynchronizationError::EmailMissing => "Email is missing".to_string(),
            SynchronizationError::EncryptionFailed => "Encryption failed".to_string(),
            SynchronizationError::FileNameMissing => "File name is missing".to_string(),
            SynchronizationError::HostNotFound => "Host not found".to_string(),
            SynchronizationError::InvalidResponse(e) => e.to_string(),
            SynchronizationError::MissingEncryptionKey => "Missing encryption key".to_string(),
            SynchronizationError::MissingExpirationDate => "Missing expiration date".to_string(),
            SynchronizationError::Other(e) => e.to_string(),
            SynchronizationError::ServerError(e) => e.to_string(),
            SynchronizationError::ServerUrlMissing => "Server URL is missing".to_string(),
            SynchronizationError::Timeout(timeout) => format!("Timeout: {}", timeout),
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

// ============================================================================
// SSE (Server-Sent Events) Types
// ============================================================================

/// Heartbeat event data sent by SSE to keep connection alive
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct HeartbeatData {
    pub timestamp: String,
}

/// Share notification event data sent by SSE when a file is shared
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ShareNotification {
    pub share_id: String,
    pub source_device_id: String,
    pub destination_device_id: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_hash: String,
    pub content: String, // Encrypted and base64 encoded
    pub created_at: String,
    pub expires_at: String,
}

/// SSE Event types that can be received from the server
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// Heartbeat event to keep connection alive (sent every ~30s)
    Heartbeat { timestamp: String },
    /// File share notification with full share details
    ShareAvailable(ShareNotification),
    /// Error event for connection or parsing errors
    Error(String),
}

impl SseEvent {
    /// Parse an SSE event from the event type and data
    ///
    /// ### Arguments
    /// - `event_type`: The SSE event type (e.g., "heartbeat", "share_available")
    /// - `data`: The JSON data for the event
    ///
    /// ### Returns
    /// - `SseEvent`: The parsed event
    fn parse(event_type: &str, data: &str) -> Self {
        match event_type {
            "heartbeat" => match serde_json::from_str::<HeartbeatData>(data) {
                Ok(hb) => SseEvent::Heartbeat {
                    timestamp: hb.timestamp,
                },
                Err(e) => {
                    log::warn!("Failed to parse heartbeat: {}", e);
                    SseEvent::Heartbeat {
                        timestamp: String::new(),
                    }
                }
            },
            "share_available" => match serde_json::from_str::<ShareNotification>(data) {
                Ok(notification) => SseEvent::ShareAvailable(notification),
                Err(e) => {
                    log::error!("Failed to parse share notification: {}", e);
                    SseEvent::Error(format!("Invalid share notification: {}", e))
                }
            },
            "" => {
                // No event type means generic message event
                SseEvent::Error(format!("Unknown event (no event type): {}", data))
            }
            _ => {
                log::warn!("Unknown SSE event type: {}", event_type);
                SseEvent::Error(format!("Unknown event type: {}", event_type))
            }
        }
    }
}

impl Fulgur {
    /// Check if the Fulgur is connected to the sync server
    ///
    /// ### Returns
    /// - `true` if Fulgur is connected to the sync server, `false` otherwise
    pub fn is_connected(&self) -> bool {
        self.sync_server_connection_status.lock().is_connected()
    }

    /// Restart the SSE connection with new settings
    ///
    /// ### Description
    /// Stops the current SSE connection and starts a new one with the updated settings.
    /// Should be called when synchronization settings (server URL, email, or key) change.
    pub fn restart_sse_connection(&mut self) {
        if let Some(ref shutdown_flag) = self.sse_shutdown_flag {
            log::info!("Signaling SSE connection to shutdown...");
            shutdown_flag.store(true, Ordering::Relaxed);
        }
        thread::sleep(Duration::from_millis(100));
        let (sse_tx, sse_rx) = std::sync::mpsc::channel();
        let sse_shutdown_flag = Arc::new(AtomicBool::new(false));
        self.sse_events = Some(sse_rx);
        self.sse_event_tx = Some(sse_tx.clone());
        self.sse_shutdown_flag = Some(sse_shutdown_flag.clone());
        if self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            let settings = self.settings.clone();
            let sync_status = self.sync_server_connection_status.clone();
            let token_state = Arc::clone(&self.token_state);
            thread::spawn(move || {
                // Small delay to ensure old connection is fully stopped
                thread::sleep(Duration::from_millis(200));
                match files::sync::initial_synchronization(
                    &settings.app_settings.synchronization_settings,
                    Arc::clone(&token_state),
                ) {
                    Ok(_) => {
                        log::info!("Initial sync succeeded, starting new SSE connection");
                        if let Err(e) = files::sync::connect_sse(
                            &settings.app_settings.synchronization_settings,
                            sse_tx,
                            sse_shutdown_flag,
                            sync_status,
                            token_state,
                        ) {
                            log::error!("Failed to start new SSE connection: {}", e.to_string());
                        }
                    }
                    Err(e) => {
                        log::error!("Initial sync failed, not starting SSE: {}", e.to_string());
                    }
                }
            });
        } else {
            log::info!("Synchronization is not activated, SSE connection not started");
        }
    }

    /// Handle SSE (Server-Sent Events) from the sync server
    ///
    /// ### Arguments
    /// - `event`: The SSE event to handle
    /// - `window`: The window to show notifications in
    /// - `cx`: The application context
    pub fn handle_sse_event(
        &mut self,
        event: files::sync::SseEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Debounce: ignore events within 500ms of last event
        let now = Instant::now();
        if let Some(last_time) = self.last_sse_event {
            if now.duration_since(last_time) < Duration::from_millis(500) {
                return;
            }
        }
        self.last_sse_event = Some(now);
        match event {
            files::sync::SseEvent::Heartbeat { timestamp } => {
                log::debug!("SSE heartbeat received: {}", timestamp);
                let was_disconnected = !self.sync_server_connection_status.lock().is_connected();
                {
                    let mut last_heartbeat = self.last_heartbeat.lock();
                    *last_heartbeat = Some(now);
                }
                if was_disconnected {
                    *self.sync_server_connection_status.lock() = SynchronizationStatus::Connected;
                    log::info!("Connection restored - heartbeat received after timeout");
                }
            }
            files::sync::SseEvent::ShareAvailable(notification) => {
                log::info!(
                    "File shared from device {}: {}",
                    notification.source_device_id,
                    notification.file_name
                );
                {
                    let mut files = self.pending_shared_files.lock();
                    let shared_file = fulgur_common::api::shares::SharedFileResponse {
                        id: notification.share_id,
                        source_device_id: notification.source_device_id.clone(),
                        file_name: notification.file_name.clone(),
                        file_size: notification.file_size as i32,
                        content: notification.content,
                        created_at: notification.created_at,
                        expires_at: notification.expires_at,
                    };
                    files.push(shared_file);
                }
                let message =
                    SharedString::from(format!("New file received: {}", notification.file_name));
                window.push_notification((NotificationType::Info, message), cx);
            }
            files::sync::SseEvent::Error(err) => {
                log::error!("SSE error: {}", err);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_compress_decompress() {
        let original = "This is a test string with some repetitive content. \
                       This is a test string with some repetitive content. \
                       This is a test string with some repetitive content.";

        // Compress the content
        let compressed = compress_content(original).expect("Compression should succeed");

        // Compressed should be smaller than original
        assert!(compressed.len() < original.len());

        // Decompress the content
        let decompressed = decompress_content(&compressed).expect("Decompression should succeed");

        // Decompressed should match original
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_compress_empty_string() {
        let original = "";

        let compressed = compress_content(original).expect("Compression should succeed");
        let decompressed = decompress_content(&compressed).expect("Decompression should succeed");

        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_compress_small_content() {
        // Small content might not compress well, but should still work
        let original = "Hi!";

        let compressed = compress_content(original).expect("Compression should succeed");
        let decompressed = decompress_content(&compressed).expect("Decompression should succeed");

        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_compress_unicode() {
        let original = "Hello 世界! 🚀 Testing unicode compression.";

        let compressed = compress_content(original).expect("Compression should succeed");
        let decompressed = decompress_content(&compressed).expect("Decompression should succeed");

        assert_eq!(decompressed, original);
    }
}
