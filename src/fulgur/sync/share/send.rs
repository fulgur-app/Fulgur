use super::{
    compression::compress_content,
    devices::Device,
    types::{ShareFileRequest, ShareResult},
};
use crate::fulgur::{
    settings::SynchronizationSettings,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{SynchronizationError, handle_ureq_error},
    },
    utils::crypto_helper::{self, is_valid_public_key},
};
use fulgur_common::api::shares::ShareFilePayload;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const MAX_SYNC_SHARE_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Encrypt compressed content for a specific device
///
/// ### Arguments
/// - `compressed_content`: The content to encrypt
/// - `device_public_key`: The device's public key for encryption
///
/// ### Returns
/// - `Ok(String)`: The encrypted and compressed content (base64-encoded)
/// - `Err(SynchronizationError)`: If encryption failed
fn encrypt_content_for_device(
    compressed_content: &[u8],
    device_public_key: &str,
) -> Result<String, SynchronizationError> {
    crypto_helper::encrypt_bytes(compressed_content, device_public_key).map_err(|e| {
        log::error!("Failed to encrypt content: {e}");
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
        .header("Authorization", &format!("Bearer {token}"))
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload)
        .map_err(|e| {
            handle_ureq_error(e, &format!("Failed to share file to device {device_id}"))
        })?;
    let body = response.body_mut().read_to_string().map_err(|e| {
        log::error!("Failed to read response body: {e}");
        SynchronizationError::InvalidResponse(e.to_string())
    })?;
    let json: serde_json::Value = serde_json::from_str(&body).map_err(|e| {
        log::error!("Failed to parse response body: {e}");
        SynchronizationError::InvalidResponse(e.to_string())
    })?;
    let expiration_date = json["expiration_date"]
        .as_str()
        .ok_or(SynchronizationError::MissingExpirationDate)?;
    log::info!("File shared successfully to device {device_id} until {expiration_date}");
    Ok(expiration_date.to_string())
}

/// Share a file with multiple devices (per-device encryption)
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `request`: The share request (content, file name, target device IDs, optional path)
/// - `devices`: The list of all devices (with their public keys)
/// - `token_state`: Thread-safe token state manager (with condition variable)
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `max_file_size_bytes`: Server-advertised maximum file size; `u64::MAX` means no limit
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
    max_file_size_bytes: u64,
) -> Result<ShareResult, SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .as_ref()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    if request.content.is_empty() {
        return Err(SynchronizationError::ContentMissing);
    }
    if max_file_size_bytes != u64::MAX {
        let max_size = max_file_size_bytes as usize;
        if request.content.len() > max_size {
            return Err(SynchronizationError::ContentTooLarge {
                file_size: request.content.len(),
                max_size,
            });
        }
    }
    if request.file_name.is_empty() {
        return Err(SynchronizationError::FileNameMissing);
    }
    if request.device_ids.is_empty() {
        return Err(SynchronizationError::DeviceIdsMissing);
    }
    for device_id in &request.device_ids {
        let device = devices
            .iter()
            .find(|d| d.id == *device_id)
            .ok_or_else(|| SynchronizationError::Other(format!("Device {device_id} not found")))?;
        match &device.public_key {
            None => return Err(SynchronizationError::MissingPublicKey(device.name.clone())),
            Some(key) if !is_valid_public_key(key) => {
                return Err(SynchronizationError::InvalidPublicKey(device.name.clone()));
            }
            _ => {}
        }
    }
    let token = get_valid_token(
        synchronization_settings,
        Arc::clone(&token_state),
        http_agent,
    )?;
    let share_url = format!("{server_url}/api/share");
    let deduplication_hash = if synchronization_settings.is_deduplication {
        request.file_path.as_ref().map(|path| {
            let hash = Sha256::digest(path.to_string_lossy().as_bytes());
            format!("{hash:x}")
        })
    } else {
        None
    };
    let compressed_content = match compress_content(&request.content) {
        Ok(c) => c,
        Err(e) => {
            log::error!("Failed to compress content: {e}");
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
                                log::warn!("Device {device_id} not found, skipping");
                                return (
                                    device_id.clone(),
                                    Err(SynchronizationError::Other(format!(
                                        "Device {device_id} not found"
                                    ))),
                                );
                            }
                        };
                        let public_key = match &device.public_key {
                            Some(key) => key,
                            None => {
                                log::warn!("Device {device_id} has no public key, skipping");
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
                                        "Failed to encrypt content for device {device_id}: {e}"
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
                                log::error!("Failed to share file to device {device_id}: {e}");
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
                        log::error!("Share thread panicked: {e:?}");
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
    use super::{MAX_SYNC_SHARE_PAYLOAD_BYTES, share_file};
    use crate::fulgur::settings::SynchronizationSettings;
    use crate::fulgur::sync::share::{Device, ShareFileRequest};
    use crate::fulgur::sync::{
        access_token::TokenStateManager, synchronization::SynchronizationError,
    };
    use crate::fulgur::utils::crypto_helper::generate_key_pair;
    use std::sync::Arc;

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
            name: format!("{id}-name"),
            device_type: device_type.to_string(),
            public_key: public_key.map(str::to_string),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    fn make_basic_request(device_id: &str) -> ShareFileRequest {
        ShareFileRequest {
            content: Arc::from("hello world"),
            file_name: "test.txt".to_string(),
            device_ids: vec![device_id.to_string()],
            file_path: None,
        }
    }

    // ========== share_file content size guards ==========

    #[test]
    fn test_share_file_rejects_content_larger_than_limit() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());

        let request = ShareFileRequest {
            content: Arc::from("A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES + 1)),
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        );

        assert!(matches!(
            result,
            Err(SynchronizationError::ContentTooLarge { .. })
        ));
    }

    #[test]
    fn test_share_file_accepts_content_at_exact_limit() {
        let mut settings = SynchronizationSettings::new();
        settings.server_url = Some("https://example.com".to_string());
        // Keep email unset so the flow fails deterministically at validation/network setup,
        // after content-size checks have already passed.
        settings.email = None;

        let request = ShareFileRequest {
            content: Arc::from("A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES)),
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        );

        assert!(
            !matches!(result, Err(SynchronizationError::ContentTooLarge { .. })),
            "Payload at exact limit should not be rejected as too large"
        );
    }

    // ========== share_file validation guards ==========

    #[test]
    fn test_share_file_rejects_missing_server_url() {
        let settings = SynchronizationSettings::new(); // server_url = None
        let request = ShareFileRequest {
            content: Arc::from("hello"),
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
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
            content: Arc::from(""),
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
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
            content: Arc::from("hello"),
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
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
            content: Arc::from("hello"),
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
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
    fn test_share_file_fails_for_unknown_device_id() {
        // Upfront validation: an unknown device ID must fail the whole request.
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let err = share_file(
            &settings,
            make_basic_request("nonexistent-device"),
            &[], // no devices provided
            token_manager,
            &make_http_agent(),
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        )
        .expect_err("share_file should fail for unknown device ID");
        assert!(
            matches!(err, SynchronizationError::Other(_)),
            "Unknown device should produce Other error, got: {err:?}"
        );
    }

    #[test]
    fn test_share_file_fails_when_device_has_no_public_key() {
        // Upfront validation: a device with no public key must fail before any work.
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device = make_device("device-no-key", "desktop", None);
        let err = share_file(
            &settings,
            make_basic_request("device-no-key"),
            &[device],
            token_manager,
            &make_http_agent(),
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        )
        .expect_err("share_file should fail when device has no public key");
        assert!(
            matches!(err, SynchronizationError::MissingPublicKey(_)),
            "Device without public key should produce MissingPublicKey, got: {err:?}"
        );
    }

    #[test]
    fn test_share_file_fails_when_device_has_invalid_public_key() {
        // Upfront validation: a malformed key must fail before encryption is attempted.
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device = make_device(
            "device-bad-key",
            "laptop",
            Some("not-a-valid-age-public-key"),
        );
        let err = share_file(
            &settings,
            make_basic_request("device-bad-key"),
            &[device],
            token_manager,
            &make_http_agent(),
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        )
        .expect_err("share_file should fail for a device with an invalid public key");
        assert!(
            matches!(err, SynchronizationError::InvalidPublicKey(_)),
            "Invalid public key should produce InvalidPublicKey, got: {err:?}"
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
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
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
                    | SynchronizationError::InvalidPublicKey(_)
                    | SynchronizationError::ContentMissing
                    | SynchronizationError::ContentTooLarge { .. }
            ),
            "Expected a network-level error, got: {:?}",
            result.failures[0].1
        );
    }

    #[test]
    fn test_share_file_fails_fast_on_first_invalid_device() {
        // Upfront validation iterates device_ids in order; the first invalid one
        // fails the whole request before any compression or encryption occurs.
        let settings = make_settings_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device_no_key = make_device("device-no-key", "desktop", None);
        let request = ShareFileRequest {
            content: Arc::from("shared content"),
            file_name: "multi.txt".to_string(),
            device_ids: vec!["device-missing".to_string(), "device-no-key".to_string()],
            file_path: None,
        };
        let err = share_file(
            &settings,
            request,
            &[device_no_key],
            token_manager,
            &make_http_agent(),
            MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        )
        .expect_err("share_file should fail on the first invalid device");
        assert!(
            matches!(err, SynchronizationError::Other(_)),
            "First unknown device should produce Other error, got: {err:?}"
        );
    }
}
