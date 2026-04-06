use crate::fulgur::{
    settings::SynchronizationSettings,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{SynchronizationError, handle_ureq_error},
    },
    ui::icons::CustomIcon,
    utils::crypto_helper,
};
use flate2::{
    Compression,
    read::{GzDecoder, GzEncoder},
};
use fulgur_common::api::{devices::DeviceResponse, shares::ShareFilePayload};
use gpui_component::Icon;
use sha2::{Digest, Sha256};
use std::{io::Read, path::PathBuf, sync::Arc};

pub type Device = DeviceResponse;
pub const MAX_SYNC_SHARE_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Maximum allowed size for decompressed content (2x the compressed payload limit).
const MAX_DECOMPRESSED_BYTES: usize = 2 * MAX_SYNC_SHARE_PAYLOAD_BYTES;

/// Parameters for sharing a file
pub struct ShareFileRequest {
    pub content: String,
    pub file_name: String,
    pub device_ids: Vec<String>,
    pub file_path: Option<PathBuf>,
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
    let mut encoder = GzEncoder::new(content.as_bytes(), Compression::default());
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
    if compressed.len() > MAX_SYNC_SHARE_PAYLOAD_BYTES {
        return Err(anyhow::anyhow!(
            "Compressed payload exceeds {} bytes limit",
            MAX_SYNC_SHARE_PAYLOAD_BYTES
        ));
    }
    let decoder = GzDecoder::new(compressed);
    let mut limited_reader = decoder.take((MAX_DECOMPRESSED_BYTES + 1) as u64);
    let mut decompressed_bytes = Vec::with_capacity(MAX_SYNC_SHARE_PAYLOAD_BYTES);
    limited_reader.read_to_end(&mut decompressed_bytes)?;
    if decompressed_bytes.len() > MAX_DECOMPRESSED_BYTES {
        return Err(anyhow::anyhow!(
            "Decompressed payload exceeds {} bytes limit",
            MAX_DECOMPRESSED_BYTES
        ));
    }
    let decompressed = String::from_utf8(decompressed_bytes)
        .map_err(|e| anyhow::anyhow!("Failed to decode decompressed content as UTF-8: {}", e))?;
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
/// - `token_state`: Arc to the token state manager (thread-safe with condition variable)
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(Vec<Device>)`: The devices
/// - `Err(SynchronizationError)`: If the devices could not be retrieved
pub fn get_devices(
    synchronization_settings: &SynchronizationSettings,
    token_state: Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<Vec<Device>, SynchronizationError> {
    let Some(server_url) = synchronization_settings.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(synchronization_settings, token_state, http_agent)?;
    let devices_url = format!("{}/api/devices", server_url);
    let mut response = http_agent
        .get(&devices_url)
        .header("Authorization", &format!("Bearer {}", token))
        .call()
        .map_err(|e| handle_ureq_error(e, "Failed to get devices"))?;

    let devices: Vec<Device> = response
        .body_mut()
        .read_json::<Vec<Device>>()
        .map_err(|e| {
            log::error!("Failed to read devices: {}", e);
            SynchronizationError::InvalidResponse(e.to_string())
        })?;

    log::debug!("Retrieved {} devices from server", devices.len());
    Ok(devices)
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
/// - `http_agent`: Shared HTTP agent for connection pooling
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
    http_agent: &ureq::Agent,
) -> Result<String, SynchronizationError> {
    let encrypted_payload = ShareFilePayload {
        content: encrypted_content,
        file_name: file_name.to_string(),
        device_id: device_id.to_string(),
        deduplication_hash,
    };
    let mut response = http_agent
        .post(share_url)
        .header("Authorization", &format!("Bearer {}", token))
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload)
        .map_err(|e| {
            handle_ureq_error(e, &format!("Failed to share file to device {}", device_id))
        })?;
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
/// - `token_state`: Thread-safe token state manager (with condition variable)
/// - `file_path`: Optional file path (used for deduplication hash)
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(ShareResult)`: Results of sharing with each device
/// - `Err(SynchronizationError)`: If validation or setup failed
pub fn share_file(
    synchronization_settings: &SynchronizationSettings,
    request: ShareFileRequest,
    devices: &[Device],
    token_state: Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<ShareResult, SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .as_ref()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    if request.content.is_empty() {
        return Err(SynchronizationError::ContentMissing);
    }
    if request.content.len() > MAX_SYNC_SHARE_PAYLOAD_BYTES {
        return Err(SynchronizationError::ContentTooLarge);
    }
    if request.file_name.is_empty() {
        return Err(SynchronizationError::FileNameMissing);
    }
    if request.device_ids.is_empty() {
        return Err(SynchronizationError::DeviceIdsMissing);
    }
    let token = get_valid_token(
        synchronization_settings,
        Arc::clone(&token_state),
        http_agent,
    )?;
    let share_url = format!("{}/api/share", server_url);
    let deduplication_hash = if synchronization_settings.is_deduplication {
        request.file_path.as_ref().map(|path| {
            let hash = Sha256::digest(path.to_string_lossy().as_bytes());
            format!("{:x}", hash)
        })
    } else {
        None
    };
    let compressed_content = match compress_content(&request.content) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to compress content: {}", e);
            return Err(SynchronizationError::CompressionFailed);
        }
    };
    // Parallelize encryption and HTTP requests across devices
    let results: Vec<(String, Result<String, SynchronizationError>)> =
        std::thread::scope(|scope| {
            let handles: Vec<_> = request
                .device_ids
                .iter()
                .map(|device_id| {
                    let device_id = device_id.clone();
                    let share_url = share_url.clone();
                    let token = token.clone();
                    let file_name = request.file_name.clone();
                    let deduplication_hash = deduplication_hash.clone();
                    let compressed_content = compressed_content.clone();
                    scope.spawn(move || {
                        let device = match devices.iter().find(|d| d.id == device_id) {
                            Some(d) => d,
                            None => {
                                log::warn!("Device {} not found, skipping", device_id);
                                return (
                                    device_id.clone(),
                                    Err(SynchronizationError::Other(format!(
                                        "Device {} not found",
                                        device_id
                                    ))),
                                );
                            }
                        };
                        let public_key = match &device.public_key {
                            Some(key) => key,
                            None => {
                                log::warn!("Device {} has no public key, skipping", device_id);
                                return (
                                    device_id.clone(),
                                    Err(SynchronizationError::MissingPublicKey(
                                        device.name.clone(),
                                    )),
                                );
                            }
                        };
                        let encrypted_content =
                            match encrypt_content_for_device(&compressed_content, public_key) {
                                Ok(content) => content,
                                Err(e) => {
                                    log::error!(
                                        "Failed to encrypt content for device {}: {}",
                                        device_id,
                                        e
                                    );
                                    return (device_id.clone(), Err(e));
                                }
                            };
                        let result = send_share_request(
                            &share_url,
                            &token,
                            encrypted_content,
                            &file_name,
                            &device_id,
                            deduplication_hash.clone(),
                            http_agent,
                        );
                        match &result {
                            Ok(_) => {}
                            Err(e) => {
                                log::error!("Failed to share file to device {}: {}", device_id, e);
                            }
                        }
                        (device_id.clone(), result)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|e| {
                        log::error!("Share thread panicked: {:?}", e);
                        (
                            String::new(),
                            Err(SynchronizationError::Other("Internal issue".to_string())),
                        )
                    })
                })
                .collect()
        });
    let mut successes = Vec::new();
    let mut failures = Vec::new();
    for (device_id, result) in results {
        match result {
            Ok(expiration_date) => {
                successes.push((device_id, expiration_date));
            }
            Err(e) => {
                failures.push((device_id, e));
            }
        }
    }
    log::info!(
        "File '{}' shared: {} succeeded, {} failed",
        request.file_name,
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
    use super::{
        Device, MAX_DECOMPRESSED_BYTES, MAX_SYNC_SHARE_PAYLOAD_BYTES, ShareFileRequest,
        ShareResult, compress_content, decompress_content, get_devices, share_file,
    };
    use crate::fulgur::settings::SynchronizationSettings;
    use crate::fulgur::sync::{
        access_token::TokenStateManager, synchronization::SynchronizationError,
    };
    use crate::fulgur::utils::crypto_helper::generate_key_pair;
    use std::sync::Arc;

    // ---------------------------------------------------------------------------
    // Test helpers
    // ---------------------------------------------------------------------------

    fn make_http_agent() -> ureq::Agent {
        ureq::Agent::new_with_config(ureq::config::Config::builder().build())
    }

    /// Build a `SynchronizationSettings` whose server URL points at a port that
    /// is guaranteed to refuse connections immediately (no real network needed).
    fn make_settings_with_server_url() -> SynchronizationSettings {
        let mut s = SynchronizationSettings::new();
        s.server_url = Some("http://127.0.0.1:19999".to_string());
        s
    }

    fn make_token_manager_with_valid_token() -> Arc<TokenStateManager> {
        let manager = Arc::new(TokenStateManager::new());
        let expires_at = time::OffsetDateTime::now_utc() + time::Duration::hours(1);
        manager.inject_token_for_test("test-jwt-token".to_string(), expires_at);
        manager
    }

    fn make_device(id: &str, device_type: &str, public_key: Option<&str>) -> Device {
        Device {
            id: id.to_string(),
            name: format!("{}-name", id),
            device_type: device_type.to_string(),
            public_key: public_key.map(str::to_string),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_basic_request(device_id: &str) -> ShareFileRequest {
        ShareFileRequest {
            content: "hello world".to_string(),
            file_name: "test.txt".to_string(),
            device_ids: vec![device_id.to_string()],
            file_path: None,
        }
    }

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

    #[test]
    fn test_decompress_rejects_oversized_compressed_payload() {
        let oversized_payload = vec![0_u8; MAX_SYNC_SHARE_PAYLOAD_BYTES + 1];
        let result = decompress_content(&oversized_payload);
        assert!(result.is_err());
    }

    #[test]
    fn test_decompress_rejects_oversized_decompressed_payload() {
        let original = "A".repeat(MAX_DECOMPRESSED_BYTES + 1);
        let compressed = compress_content(&original).unwrap();
        let result = decompress_content(&compressed);
        assert!(result.is_err());
    }

    #[test]
    fn test_share_file_rejects_content_larger_than_limit() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());

        let request = ShareFileRequest {
            content: "A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES + 1),
            file_name: "large.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };

        let result = share_file(
            &settings,
            request,
            &[],
            Arc::new(TokenStateManager::new()),
            &ureq::Agent::new_with_config(ureq::config::Config::default()),
        );

        assert!(matches!(result, Err(SynchronizationError::ContentTooLarge)));
    }

    #[test]
    fn test_share_file_accepts_content_at_exact_limit() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());
        // Keep email unset so the flow fails deterministically at validation/network setup,
        // after content-size checks have already passed.
        settings.email = None;

        let request = ShareFileRequest {
            content: "A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES),
            file_name: "max-size.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };

        let result = share_file(
            &settings,
            request,
            &[],
            Arc::new(TokenStateManager::new()),
            &ureq::Agent::new_with_config(ureq::config::Config::default()),
        );

        assert!(
            !matches!(result, Err(SynchronizationError::ContentTooLarge)),
            "Payload at exact limit should not be rejected as too large"
        );
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

    #[test]
    fn test_share_result_summary_message_single_success() {
        let result = ShareResult {
            successes: vec![("device1".to_string(), "2025-06-30".to_string())],
            failures: vec![],
        };
        let message = result.summary_message();
        assert!(message.contains("File shared successfully"));
        assert!(message.contains("1 device(s)"));
        assert!(message.contains("2025-06-30"));
    }

    // ========== share_file validation guards ==========

    #[test]
    fn test_share_file_rejects_missing_server_url() {
        let settings = SynchronizationSettings::new(); // server_url = None
        let request = ShareFileRequest {
            content: "hello".to_string(),
            file_name: "test.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };
        let result = share_file(
            &settings,
            request,
            &[],
            Arc::new(TokenStateManager::new()),
            &make_http_agent(),
        );
        assert!(
            matches!(result, Err(SynchronizationError::ServerUrlMissing)),
            "Expected ServerUrlMissing, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_share_file_rejects_empty_content() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());
        let request = ShareFileRequest {
            content: String::new(),
            file_name: "test.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };
        let result = share_file(
            &settings,
            request,
            &[],
            Arc::new(TokenStateManager::new()),
            &make_http_agent(),
        );
        assert!(
            matches!(result, Err(SynchronizationError::ContentMissing)),
            "Expected ContentMissing, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_share_file_rejects_empty_file_name() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());
        let request = ShareFileRequest {
            content: "hello".to_string(),
            file_name: String::new(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };
        let result = share_file(
            &settings,
            request,
            &[],
            Arc::new(TokenStateManager::new()),
            &make_http_agent(),
        );
        assert!(
            matches!(result, Err(SynchronizationError::FileNameMissing)),
            "Expected FileNameMissing, got: {:?}",
            result.err()
        );
    }

    #[test]
    fn test_share_file_rejects_empty_device_ids() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());
        let request = ShareFileRequest {
            content: "hello".to_string(),
            file_name: "test.txt".to_string(),
            device_ids: vec![],
            file_path: None,
        };
        let result = share_file(
            &settings,
            request,
            &[],
            Arc::new(TokenStateManager::new()),
            &make_http_agent(),
        );
        assert!(
            matches!(result, Err(SynchronizationError::DeviceIdsMissing)),
            "Expected DeviceIdsMissing, got: {:?}",
            result.err()
        );
    }

    // ========== share_file per-device execution paths ==========
    // These tests inject a valid cached token to bypass the network token-refresh
    // and exercise the per-device branches inside the scoped thread pool.

    #[test]
    fn test_share_file_records_failure_for_unknown_device_id() {
        // device_ids contains an ID that is absent from the devices slice.
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let result = share_file(
            &settings,
            make_basic_request("nonexistent-device"),
            &[], // no devices provided
            token_manager,
            &make_http_agent(),
        )
        .expect("share_file should return Ok(ShareResult) even on per-device failure");
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].0, "nonexistent-device");
        assert!(
            matches!(result.failures[0].1, SynchronizationError::Other(_)),
            "Unknown device should produce Other error, got: {:?}",
            result.failures[0].1
        );
    }

    #[test]
    fn test_share_file_records_failure_when_device_has_no_public_key() {
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device = make_device("device-no-key", "desktop", None);
        let result = share_file(
            &settings,
            make_basic_request("device-no-key"),
            &[device],
            token_manager,
            &make_http_agent(),
        )
        .expect("share_file should return Ok(ShareResult)");
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 1);
        assert!(
            matches!(
                result.failures[0].1,
                SynchronizationError::MissingPublicKey(_)
            ),
            "Device without public key should produce MissingPublicKey, got: {:?}",
            result.failures[0].1
        );
    }

    #[test]
    fn test_share_file_records_failure_when_device_has_invalid_public_key() {
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device = make_device(
            "device-bad-key",
            "laptop",
            Some("not-a-valid-age-public-key"),
        );
        let result = share_file(
            &settings,
            make_basic_request("device-bad-key"),
            &[device],
            token_manager,
            &make_http_agent(),
        )
        .expect("share_file should return Ok(ShareResult)");
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 1);
        assert!(
            matches!(result.failures[0].1, SynchronizationError::EncryptionFailed),
            "Invalid public key should produce EncryptionFailed, got: {:?}",
            result.failures[0].1
        );
    }

    #[test]
    fn test_share_file_with_valid_device_key_fails_at_network() {
        // Encryption succeeds; the failure comes from the network call to a
        // non-listening port (ConnectionRefused, immediate).
        let settings = make_settings_with_server_url(); // 127.0.0.1:19999
        let token_manager = make_token_manager_with_valid_token();
        let (_, public_key) = generate_key_pair();
        let device = make_device("device-valid-key", "server", Some(&public_key.to_string()));
        let result = share_file(
            &settings,
            make_basic_request("device-valid-key"),
            &[device],
            token_manager,
            &make_http_agent(),
        )
        .expect("share_file returns Ok(ShareResult) even when per-device send fails");
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 1);
        assert_eq!(result.failures[0].0, "device-valid-key");
        // The error must be network-level, not a validation or crypto error.
        assert!(
            !matches!(
                result.failures[0].1,
                SynchronizationError::EncryptionFailed
                    | SynchronizationError::MissingPublicKey(_)
                    | SynchronizationError::ContentMissing
                    | SynchronizationError::ContentTooLarge
            ),
            "Expected a network-level error, got: {:?}",
            result.failures[0].1
        );
    }

    #[test]
    fn test_share_file_multi_device_collects_all_failures() {
        // Two device IDs: one absent from the slice, one present but without a key.
        // Both should land in failures; the result is still Ok(ShareResult).
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device_no_key = make_device("device-no-key", "desktop", None);
        let request = ShareFileRequest {
            content: "shared content".to_string(),
            file_name: "multi.txt".to_string(),
            device_ids: vec!["device-missing".to_string(), "device-no-key".to_string()],
            file_path: None,
        };
        let result = share_file(
            &settings,
            request,
            &[device_no_key],
            token_manager,
            &make_http_agent(),
        )
        .expect("share_file should return Ok(ShareResult)");
        assert_eq!(result.successes.len(), 0);
        assert_eq!(result.failures.len(), 2);
        assert!(!result.is_complete_success());
    }

    // ========== get_devices validation ==========

    #[test]
    fn test_get_devices_fails_without_server_url() {
        let settings = SynchronizationSettings::new(); // server_url = None
        let result = get_devices(
            &settings,
            Arc::new(TokenStateManager::new()),
            &make_http_agent(),
        );
        assert!(
            matches!(result, Err(SynchronizationError::ServerUrlMissing)),
            "Expected ServerUrlMissing, got: {:?}",
            result.err()
        );
    }
}
