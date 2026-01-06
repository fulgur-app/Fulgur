use std::{sync::atomic::Ordering, thread, time::Duration};

use crate::fulgur::{crypto_helper, ui::icons::CustomIcon};
use flate2::Compression;
use flate2::read::{GzDecoder, GzEncoder};
use fulgur_common::api::BeginResponse;
use fulgur_common::api::devices::DeviceResponse;
use gpui::{App, Entity, SharedString};
use gpui_component::Icon;
use gpui_component::notification::NotificationType;
use serde::Serialize;
use std::io::Read;

pub type Device = DeviceResponse;

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

/// Get the devices from the server
///
/// ### Arguments
/// - `server_url`: The server URL
/// - `email`: The email
/// - `key`: The key
///
/// ### Returns
/// - `Ok(Vec<Device>)`: The devices
/// - `Err(anyhow::Error)`: If the devices could not be retrieved
pub fn get_devices(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
) -> Result<Vec<Device>, SynchronizationError> {
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
    let devices_url = format!("{}/api/devices", server_url.unwrap());
    let response = ureq::get(&devices_url)
        .header("Authorization", &format!("Bearer {}", decrypted_key))
        .header("X-User-Email", &email.unwrap())
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
/// - `Err(anyhow::Error)`: If the encryption key could not be fetched
fn fetch_encryption_key(server_url: &str, email: &str, device_key: &str) -> Result<String, SynchronizationError> {
    let key_url = format!("{}/api/encryption-key", server_url);
    let mut response = match ureq::get(&key_url)
        .header("Authorization", &format!("Bearer {}", device_key))
        .header("X-User-Email", email)
        .call() {
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
                    std::io::ErrorKind::TimedOut => {
                        Err(SynchronizationError::ConnectionFailed)
                    }
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
/// - `server_url`: The server URL
/// - `email`: The email
/// - `key`: The encrypted device authentication key
/// - `payload`: The payload to share the file with (content will be encrypted)
///
/// ### Returns
/// - `Ok(String)`: The expiration date of the shared file
/// - `Err(anyhow::Error)`: If the file could not be shared
pub fn share_file(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
    payload: ShareFilePayload,
) -> Result<String, SynchronizationError> {
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    if email.is_none() {
        return Err(SynchronizationError::EmailMissing);
    }
    if key.is_none() {
        return Err(SynchronizationError::DeviceKeyMissing);
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
    let server_url_str = server_url.as_ref().unwrap();
    let email_str = email.as_ref().unwrap();
    let decrypted_device_key = match crypto_helper::decrypt(&key.unwrap()) {
        Ok(key) => key,
        Err(e) => {
            log::error!("Failed to decrypt device key: {}", e);
            return Err(SynchronizationError::EncryptedKeyDecryptionFailed);
        }
    };
    let encryption_key = fetch_encryption_key(server_url_str, email_str, &decrypted_device_key)?;
    let compressed_content = match compress_content(&payload.content) {
        Ok(content) => content,
        Err(e) => {
            log::error!("Failed to compress content: {}", e);
            return Err(SynchronizationError::CompressionFailed);
        }
    };
    let encrypted_content = match crypto_helper::encrypt_bytes(&compressed_content, &encryption_key) {
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
        .header("Authorization", &format!("Bearer {}", decrypted_device_key))
        .header("X-User-Email", email_str)
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload) {
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
        Err(SynchronizationError::ServerError(response.status().as_u16()))
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
/// - `Err(anyhow::Error)`: If the synchronization could not be performed
pub fn initial_synchronization(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
) -> Result<BeginResponse, SynchronizationError> {
    if server_url.is_none() {
        return Err(SynchronizationError::ServerUrlMissing);
    }
    if email.is_none() {
        return Err(SynchronizationError::EmailMissing);
    }
    if key.is_none() {
        return Err(SynchronizationError::DeviceKeyMissing);
    }
    let server_url_str = server_url.as_ref().unwrap();
    let email_str = email.as_ref().unwrap();
    let decrypted_device_key = match crypto_helper::decrypt(&key.unwrap()) {
        Ok(key) => key,
        Err(e) => {
            log::error!("Failed to decrypt device key: {}", e);
            return Err(SynchronizationError::EncryptedKeyDecryptionFailed);
        }
    };
    let begin_url = format!("{}/api/begin", server_url_str);
    let mut response = match ureq::get(&begin_url)
        .header("Authorization", &format!("Bearer {}", decrypted_device_key))
        .header("X-User-Email", email_str)
        .call() {
            Ok(response) => response,
            Err(ureq::Error::StatusCode(code)) => {
                // HTTP status errors (401, 403, 404, 500, etc.)
                log::error!("Failed to begin synchronization: HTTP status {}", code);
                if code == 401 || code == 403 {
                    return Err(SynchronizationError::AuthenticationFailed);
                } else {
                    return Err(SynchronizationError::ServerError(code));
                }
            }
            Err(ureq::Error::Io(io_error)) => {
                // IO/connection errors
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
                    std::io::ErrorKind::AddrNotAvailable => {
                        Err(SynchronizationError::HostNotFound)
                    }
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
    let is_connected = entity.read(cx).is_connected.clone();
    let pending_shared_files = entity.read(cx).pending_shared_files.clone();
    let encryption_key = entity.read(cx).encryption_key.clone();
    let device_name = entity.read(cx).device_name.clone();
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
            is_connected.store(false, std::sync::atomic::Ordering::Relaxed);
            return;
        }
        match initial_synchronization(server_url, email, key) {
            Ok(begin_response) => {
                log::info!("Successfully connected to sync server");
                is_connected.store(true, std::sync::atomic::Ordering::Relaxed);
                if let Ok(mut key) = encryption_key.lock() {
                    *key = Some(begin_response.encryption_key);
                }
                if let Ok(mut device_name) = device_name.lock() {
                    *device_name = Some(begin_response.device_name);
                }
                if let Ok(mut files) = pending_shared_files.lock() {
                    *files = begin_response.shares;
                }
            }
            Err(e) => {
                log::error!("Failed to fetch shared files: {}", e.to_string());
                is_connected.store(false, Ordering::Relaxed);
            }
        }
    });
}

/// Perform initial synchronization with the server
///
/// This function reads the synchronization settings, attempts to connect to the server,
/// and updates the application state accordingly. Stores notification to be shown on next render.
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `cx`: The context
pub fn perform_initial_synchronization(entity: Entity<crate::fulgur::Fulgur>, cx: &mut App) {
    let settings = entity.read(cx).settings.clone();
    let server_url = settings
        .app_settings
        .synchronization_settings
        .server_url
        .clone();
    let email = settings
        .app_settings
        .synchronization_settings
        .email
        .clone();
    let key = settings
        .app_settings
        .synchronization_settings
        .key
        .clone();

    let result = initial_synchronization(server_url, email, key);

    let (notification, is_connected) = match result {
        Ok(begin_response) => {
            // Update encryption key and device name
            entity.update(cx, |this, _cx| {
                if let Ok(mut key) = this.encryption_key.lock() {
                    *key = Some(begin_response.encryption_key);
                }
                if let Ok(mut name) = this.device_name.lock() {
                    *name = Some(begin_response.device_name.clone());
                }
                if let Ok(mut files) = this.pending_shared_files.lock() {
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
                true,
            )
        }
        Err(e) => (
            (
                NotificationType::Error,
                SharedString::from(format!("Connection failed: {}", e.to_string())),
            ),
            false,
        ),
    };

    entity.update(cx, |this, _cx| {
        this.is_connected
            .store(is_connected, std::sync::atomic::Ordering::Relaxed);
        // Store notification to be displayed on next render
        this.pending_notification = Some(notification);
    });
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
    EncryptedKeyDecryptionFailed,
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
            SynchronizationError::EncryptedKeyDecryptionFailed => "Encrypted key decryption failed".to_string(),
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
