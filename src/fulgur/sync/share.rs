use std::{io::Read, path::PathBuf, sync::Arc};

use sha2::{Digest, Sha256};

use flate2::{
    Compression,
    read::{GzDecoder, GzEncoder},
};
use fulgur_common::api::{devices::DeviceResponse, shares::ShareFilePayload};
use gpui_component::Icon;
use parking_lot::Mutex;

use crate::fulgur::{
    settings::SynchronizationSettings,
    sync::{
        access_token::{TokenState, get_valid_token},
        synchronization::SynchronizationError,
    },
    ui::icons::CustomIcon,
    utils::crypto_helper,
};

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
    let Some(server_url) = synchronization_settings.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(synchronization_settings, token_state)?;
    let devices_url = format!("{}/api/devices", server_url);
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

/// Encrypt and compress content for a specific device
///
/// ### Arguments
/// - `compressed_content`: The content to encrypt
/// - `device_public_key`: The device's public key for encryption
///
/// ### Returns
/// - `Ok(String)`: The encrypted and compressed content (base64-encoded)
/// - `Err(SynchronizationError)`: If encryption or compression failed
fn encrypt_content_for_device(
    compressed_content: &[u8],
    device_public_key: &str,
) -> Result<String, SynchronizationError> {
    crypto_helper::encrypt_bytes(compressed_content, device_public_key).map_err(|e| {
        log::error!("Failed to encrypt content: {}", e);
        SynchronizationError::EncryptionFailed
    })
}

/// Send a share request to the server for a single device
///
/// ### Arguments
/// - `share_url`: The share endpoint URL
/// - `token`: The authentication token
/// - `encrypted_content`: The encrypted content
/// - `file_name`: The file name
/// - `device_id`: The device ID
/// - `deduplication_hash`: Optional SHA256 hash of the file path for server-side deduplication
///
/// ### Returns
/// - `Ok(String)`: The expiration date from the response
/// - `Err(SynchronizationError)`: If the request failed
fn send_share_request(
    share_url: &str,
    token: &str,
    encrypted_content: String,
    file_name: &str,
    device_id: &str,
    deduplication_hash: Option<String>,
) -> Result<String, SynchronizationError> {
    let encrypted_payload = ShareFilePayload {
        content: encrypted_content,
        file_name: file_name.to_string(),
        device_id: device_id.to_string(),
        deduplication_hash,
    };
    let mut response = match ureq::post(share_url)
        .header("Authorization", &format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload)
    {
        Ok(response) => response,
        Err(ureq::Error::StatusCode(code)) => {
            log::error!(
                "Failed to share file to device {}: HTTP status {}",
                device_id,
                code
            );
            return if code == 401 || code == 403 {
                Err(SynchronizationError::AuthenticationFailed)
            } else if code == 400 {
                Err(SynchronizationError::BadRequest)
            } else {
                Err(SynchronizationError::ServerError(code))
            };
        }
        Err(ureq::Error::Io(io_error)) => {
            log::error!(
                "Failed to share file to device {} (IO): {}",
                device_id,
                io_error
            );
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
            log::error!(
                "Failed to share file to device {}: Connection failed",
                device_id
            );
            return Err(SynchronizationError::ConnectionFailed);
        }
        Err(ureq::Error::HostNotFound) => {
            log::error!(
                "Failed to share file to device {}: Host not found",
                device_id
            );
            return Err(SynchronizationError::HostNotFound);
        }
        Err(ureq::Error::Timeout(timeout)) => {
            log::error!(
                "Failed to share file to device {}: Timeout ({})",
                device_id,
                timeout
            );
            return Err(SynchronizationError::Timeout(timeout.to_string()));
        }
        Err(e) => {
            log::error!("Failed to share file to device {}: {}", device_id, e);
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    if response.status() == 200 {
        let body = response.body_mut().read_to_string().map_err(|e| {
            log::error!("Failed to read response body: {}", e);
            SynchronizationError::InvalidResponse(e.to_string())
        })?;
        let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
            log::error!("Failed to parse response body: {}", e);
            SynchronizationError::InvalidResponse(e.to_string())
        })?;
        let expiration_date = json["expiration_date"]
            .as_str()
            .ok_or(SynchronizationError::MissingExpirationDate)?;
        log::info!(
            "File shared successfully to device {} until {}",
            device_id,
            expiration_date
        );
        Ok(expiration_date.to_string())
    } else {
        Err(SynchronizationError::ServerError(
            response.status().as_u16(),
        ))
    }
}

/// Result of sharing a file with devices
pub struct ShareResult {
    pub successes: Vec<(String, String)>, // (device_id, expiration_date)
    pub failures: Vec<(String, SynchronizationError)>, // (device_id, error)
}

impl ShareResult {
    /// Check if all shares were successful
    ///
    /// ### Returns
    /// - `true`: If all shares were successful, `false`` otherwise
    pub fn is_complete_success(&self) -> bool {
        self.failures.is_empty()
    }

    /// Get a summary message for the share operation
    ///
    /// ### Returns
    /// - `String`: The message
    pub fn summary_message(&self) -> String {
        let total = self.successes.len() + self.failures.len();
        if self.is_complete_success() {
            if let Some((_, expiration)) = self.successes.first() {
                format!(
                    "File shared successfully to {} device(s) until {}.",
                    total, expiration
                )
            } else if total == 0 {
                "The file was not shared.".to_string()
            } else {
                "File shared successfully.".to_string()
            }
        } else if self.successes.is_empty() {
            format!("Failed to share file to all {} device(s).", total)
        } else {
            format!(
                "File shared to {}/{} device(s). {} failed.",
                self.successes.len(),
                total,
                self.failures.len()
            )
        }
    }
}

/// Share a file with multiple devices (per-device encryption)
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `content`: The content of the file
/// - `file_name`: The name of the file
/// - `device_ids`: The ids of the devices to sent the file to
/// - `devices`: The list of all devices (with their public keys)
/// - `token_state`: Thread-safe token state
/// - `file_path`: Optional file path (used for deduplication hash)
///
/// ### Returns
/// - `Ok(ShareResult)`: Results of sharing with each device
/// - `Err(SynchronizationError)`: If validation or setup failed
pub fn share_file(
    synchronization_settings: &SynchronizationSettings,
    content: String,
    file_name: String,
    device_ids: Vec<String>,
    devices: &[Device],
    token_state: Arc<Mutex<TokenState>>,
    file_path: Option<PathBuf>,
) -> Result<ShareResult, SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .as_ref()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    if content.is_empty() {
        return Err(SynchronizationError::ContentMissing);
    }
    if content.len() > 1024 * 1024 {
        return Err(SynchronizationError::ContentTooLarge);
    }
    if file_name.is_empty() {
        return Err(SynchronizationError::FileNameMissing);
    }
    if device_ids.is_empty() {
        return Err(SynchronizationError::DeviceIdsMissing);
    }
    let token = get_valid_token(synchronization_settings, Arc::clone(&token_state))?;
    let share_url = format!("{}/api/share", server_url);
    let deduplication_hash = if synchronization_settings.is_deduplication {
        file_path.as_ref().map(|path| {
            let hash = Sha256::digest(path.to_string_lossy().as_bytes());
            format!("{:x}", hash)
        })
    } else {
        None
    };
    let compressed_content = match compress_content(&content) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to compress content: {}", e);
            return Err(SynchronizationError::CompressionFailed);
        }
    };
    let mut successes = Vec::new();
    let mut failures = Vec::new();
    for device_id in &device_ids {
        let device = match devices.iter().find(|d| &d.id == device_id) {
            Some(d) => d,
            None => {
                log::warn!("Device {} not found, skipping", device_id);
                failures.push((
                    device_id.clone(),
                    SynchronizationError::Other(format!("Device {} not found", device_id)),
                ));
                continue;
            }
        };
        let public_key = match &device.public_key {
            Some(key) => key,
            None => {
                log::warn!("Device {} has no public key, skipping", device_id);
                failures.push((
                    device_id.clone(),
                    SynchronizationError::MissingPublicKey(device.name.clone()),
                ));
                continue;
            }
        };
        let encrypted_content = match encrypt_content_for_device(&compressed_content, public_key) {
            Ok(content) => content,
            Err(e) => {
                log::error!("Failed to encrypt content for device {}: {}", device_id, e);
                failures.push((device_id.clone(), e));
                continue;
            }
        };
        match send_share_request(
            &share_url,
            &token,
            encrypted_content,
            &file_name,
            device_id,
            deduplication_hash.clone(),
        ) {
            Ok(expiration_date) => {
                successes.push((device_id.clone(), expiration_date));
            }
            Err(e) => {
                log::error!("Failed to share file to device {}: {}", device_id, e);
                failures.push((device_id.clone(), e));
            }
        }
    }
    log::info!(
        "File '{}' shared: {} succeeded, {} failed",
        file_name,
        successes.len(),
        failures.len()
    );
    Ok(ShareResult {
        successes,
        failures,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    // ========== Compression Tests ==========

    #[test]
    fn test_roundtrip_empty_string() {
        let original = "";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_ascii_text() {
        let original = "The quick brown fox jumps over the lazy dog.";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_unicode_text() {
        let original = "Héllo Wörld! 你好世界 🌍 Привет";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_multiline_text() {
        let original = "Line 1\nLine 2\r\nLine 3\n\tIndented\n\nEmpty line above";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_special_characters() {
        let original = "!@#$%^&*()_+-=[]{}|;':\",./<>?`~";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_large_repetitive_content() {
        let original = "AAAA".repeat(5000);
        let compressed = compress_content(&original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
        // Should compress very well
        assert!(compressed.len() < original.len() / 10);
    }

    #[test]
    fn test_roundtrip_json_like_content() {
        let original = r#"{"name": "test", "value": 123, "nested": {"key": "value"}}"#;
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_code_like_content() {
        let original = r#"
fn main() {
    let x = 42;
    println!("Hello, world! {}", x);
}
"#;
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_mixed_unicode() {
        let original = "English, 中文, 日本語, 한국어, العربية, עברית, Ελληνικά, Русский";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_emoji_heavy() {
        let original = "🔥🎉🚀💻⚡️🌟✨🎯🌈🦄🐉🌸🍕🎮🎨🎭🎪🎬🎤🎧🎼";
        let compressed = compress_content(original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    #[test]
    fn test_roundtrip_very_large_content() {
        let original = "Lorem ipsum dolor sit amet. ".repeat(10000);
        assert!(original.len() > 250000); // Over 250KB
        let compressed = compress_content(&original).unwrap();
        let decompressed = decompress_content(&compressed).unwrap();
        assert_eq!(decompressed, original);
    }

    // ========== ShareResult Tests ==========

    #[test]
    fn test_share_result_is_complete_success_all_successful() {
        let result = ShareResult {
            successes: vec![
                ("device1".to_string(), "2025-01-01".to_string()),
                ("device2".to_string(), "2025-01-01".to_string()),
            ],
            failures: vec![],
        };
        assert!(result.is_complete_success());
    }

    #[test]
    fn test_share_result_is_complete_success_with_failures() {
        let result = ShareResult {
            successes: vec![("device1".to_string(), "2025-01-01".to_string())],
            failures: vec![(
                "device2".to_string(),
                SynchronizationError::ConnectionFailed,
            )],
        };
        assert!(!result.is_complete_success());
    }

    #[test]
    fn test_share_result_is_complete_success_all_failed() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![
                (
                    "device1".to_string(),
                    SynchronizationError::ConnectionFailed,
                ),
                (
                    "device2".to_string(),
                    SynchronizationError::AuthenticationFailed,
                ),
            ],
        };
        assert!(!result.is_complete_success());
    }

    #[test]
    fn test_share_result_summary_message_complete_success() {
        let result = ShareResult {
            successes: vec![
                ("device1".to_string(), "2025-12-31".to_string()),
                ("device2".to_string(), "2025-12-31".to_string()),
            ],
            failures: vec![],
        };
        let message = result.summary_message();
        assert!(message.contains("File shared successfully"));
        assert!(message.contains("2 device(s)"));
        assert!(message.contains("2025-12-31"));
    }

    #[test]
    fn test_share_result_summary_message_all_failed() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![
                (
                    "device1".to_string(),
                    SynchronizationError::ConnectionFailed,
                ),
                (
                    "device2".to_string(),
                    SynchronizationError::AuthenticationFailed,
                ),
            ],
        };
        let message = result.summary_message();
        assert!(message.contains("Failed to share file to all"));
        assert!(message.contains("2 device(s)"));
    }

    #[test]
    fn test_share_result_summary_message_partial_success() {
        let result = ShareResult {
            successes: vec![
                ("device1".to_string(), "2025-12-31".to_string()),
                ("device2".to_string(), "2025-12-31".to_string()),
            ],
            failures: vec![(
                "device3".to_string(),
                SynchronizationError::ConnectionFailed,
            )],
        };
        let message = result.summary_message();
        assert!(message.contains("2/3 device(s)"));
        assert!(message.contains("1 failed"));
    }

    #[test]
    fn test_share_result_summary_message_empty() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![],
        };
        let message = result.summary_message();
        assert_eq!(message, "The file was not shared.");
    }
}
