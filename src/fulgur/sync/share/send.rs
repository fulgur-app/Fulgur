use super::{
    compression::compress_content,
    devices::Device,
    types::{ShareFileRequest, ShareResult},
};
use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{MAX_HTTP_SMALL_RESPONSE_BYTES, SynchronizationError, handle_ureq_error},
    },
    utils::crypto_helper::{self, is_valid_public_key},
};
use fulgur_common::api::shares::ShareFilePayload;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const MAX_SYNC_SHARE_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Maximum number of pending shares the client will accept in a single server response (`/api/begin`, `/api/shares`).
pub const MAX_PENDING_SHARES_PER_RESPONSE: usize = 1024;

/// JSON framing overhead allowance per share (ids, filename, timestamps, quoting, separators).
pub const JSON_OVERHEAD_PER_SHARE_BYTES: usize = 1024;

/// Extract a human-readable message from a thread panic payload
///
/// ### Arguments
/// - `payload`: The boxed panic payload returned by `JoinHandle::join`
///
/// ### Returns
/// - `String`: The recovered panic message, or a placeholder if the payload is
///   neither a `&str` nor a `String`
fn panic_payload_message(payload: &(dyn std::any::Any + Send)) -> String {
    payload
        .downcast_ref::<&str>()
        .map(|s| (*s).to_string())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_else(|| "<non-string panic payload>".to_string())
}

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
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_HTTP_SMALL_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| {
            log::error!("Failed to read share response body: {e}");
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
/// - `profile`: The server profile to share through (URL, deduplication flag)
/// - `request`: The share request (content, file name, target device IDs, optional path)
/// - `devices`: The list of all devices (with their public keys)
/// - `token_state`: Per-profile token state manager (with condition variable)
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `max_file_size_bytes`: Server-advertised maximum file size; `u64::MAX` means no limit
///
/// ### Errors
/// Returns a `SynchronizationError` if the server URL is missing, the request
/// is invalid (empty content, no devices), the content exceeds the size limit,
/// or compression/encryption fails before per-device upload.
///
/// ### Returns
/// - `Ok(ShareResult)`: Results of sharing with each device
/// - `Err(SynchronizationError)`: If validation or setup failed
pub fn share_file(
    profile: &ServerProfile,
    request: &ShareFileRequest,
    devices: &[Device],
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    max_file_size_bytes: u64,
) -> Result<ShareResult, SynchronizationError> {
    let server_url = profile
        .server_url
        .as_ref()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    if request.content.is_empty() {
        return Err(SynchronizationError::ContentMissing);
    }
    if max_file_size_bytes != u64::MAX {
        let max_size = usize::try_from(max_file_size_bytes).unwrap_or(usize::MAX);
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
    let token = get_valid_token(profile, token_state, http_agent)?;
    let share_url = format!("{server_url}/api/share");
    let deduplication_hash = if profile.is_deduplication {
        request.file_path.as_ref().map(|path| {
            let hash = Sha256::digest(path.to_string_lossy().as_bytes());
            hex::encode(hash)
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
                        let Some(device) = devices.iter().find(|d| d.id == device_id) else {
                            log::warn!("Device {device_id} not found, skipping");
                            return (
                                device_id.clone(),
                                Err(SynchronizationError::Other(format!(
                                    "Device {device_id} not found"
                                ))),
                            );
                        };
                        let Some(public_key) = &device.public_key else {
                            log::warn!("Device {device_id} has no public key, skipping");
                            return (
                                device_id.clone(),
                                Err(SynchronizationError::MissingPublicKey(device.name.clone())),
                            );
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
                        let payload = panic_payload_message(e.as_ref());
                        log::error!("Share worker thread panicked: {payload}");
                        (
                            String::new(),
                            Err(SynchronizationError::Other(format!(
                                "Share worker thread panicked: {payload}"
                            ))),
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
    use crate::fulgur::settings::ServerProfile;
    use crate::fulgur::sync::share::{Device, ShareFileRequest};
    use crate::fulgur::sync::{
        access_token::TokenStateManager, synchronization::SynchronizationError,
    };
    use crate::fulgur::utils::crypto_helper::generate_key_pair;
    use std::sync::Arc;

    fn make_http_agent() -> ureq::Agent {
        ureq::Agent::new_with_config(ureq::config::Config::builder().build())
    }

    /// Build a `ServerProfile` whose server URL points at a port that
    /// is guaranteed to refuse connections immediately (no real network needed).
    fn make_profile_with_server_url() -> ServerProfile {
        let mut p = ServerProfile::new("Test");
        p.server_url = Some("http://127.0.0.1:19999".to_string());
        p
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
        let mut profile = ServerProfile::new("Test");
        profile.server_url = Some("https://example.com".to_string());

        let request = ShareFileRequest {
            content: Arc::from("A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES + 1)),
            file_name: "large.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };

        let result = share_file(
            &profile,
            &request,
            &[],
            &Arc::new(TokenStateManager::new()),
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
        let mut profile = ServerProfile::new("Test");
        profile.server_url = Some("https://example.com".to_string());
        // Keep email unset so the flow fails deterministically at validation/network setup,
        // after content-size checks have already passed.
        profile.email = None;

        let request = ShareFileRequest {
            content: Arc::from("A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES)),
            file_name: "max-size.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };

        let result = share_file(
            &profile,
            &request,
            &[],
            &Arc::new(TokenStateManager::new()),
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
        let profile = ServerProfile::new("Test"); // server_url = None
        let request = ShareFileRequest {
            content: Arc::from("hello"),
            file_name: "test.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };
        let result = share_file(
            &profile,
            &request,
            &[],
            &Arc::new(TokenStateManager::new()),
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
        let mut profile = ServerProfile::new("Test");
        profile.server_url = Some("https://example.com".to_string());
        let request = ShareFileRequest {
            content: Arc::from(""),
            file_name: "test.txt".to_string(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };
        let result = share_file(
            &profile,
            &request,
            &[],
            &Arc::new(TokenStateManager::new()),
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
        let mut profile = ServerProfile::new("Test");
        profile.server_url = Some("https://example.com".to_string());
        let request = ShareFileRequest {
            content: Arc::from("hello"),
            file_name: String::new(),
            device_ids: vec!["device-1".to_string()],
            file_path: None,
        };
        let result = share_file(
            &profile,
            &request,
            &[],
            &Arc::new(TokenStateManager::new()),
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
        let mut profile = ServerProfile::new("Test");
        profile.server_url = Some("https://example.com".to_string());
        let request = ShareFileRequest {
            content: Arc::from("hello"),
            file_name: "test.txt".to_string(),
            device_ids: vec![],
            file_path: None,
        };
        let result = share_file(
            &profile,
            &request,
            &[],
            &Arc::new(TokenStateManager::new()),
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
        let profile = make_profile_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let err = share_file(
            &profile,
            &make_basic_request("nonexistent-device"),
            &[], // no devices provided
            &token_manager,
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
        let profile = make_profile_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device = make_device("device-no-key", "desktop", None);
        let err = share_file(
            &profile,
            &make_basic_request("device-no-key"),
            &[device],
            &token_manager,
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
        let profile = make_profile_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device = make_device(
            "device-bad-key",
            "laptop",
            Some("not-a-valid-age-public-key"),
        );
        let err = share_file(
            &profile,
            &make_basic_request("device-bad-key"),
            &[device],
            &token_manager,
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
        let profile = make_profile_with_server_url(); // 127.0.0.1:19999
        let token_manager = make_token_manager_with_valid_token();
        let (_, public_key) = generate_key_pair();
        let device = make_device("device-valid-key", "server", Some(&public_key.to_string()));
        let result = share_file(
            &profile,
            &make_basic_request("device-valid-key"),
            &[device],
            &token_manager,
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
        let profile = make_profile_with_server_url();
        let token_manager = make_token_manager_with_valid_token();
        let device_no_key = make_device("device-no-key", "desktop", None);
        let request = ShareFileRequest {
            content: Arc::from("shared content"),
            file_name: "multi.txt".to_string(),
            device_ids: vec!["device-missing".to_string(), "device-no-key".to_string()],
            file_path: None,
        };
        let err = share_file(
            &profile,
            &request,
            &[device_no_key],
            &token_manager,
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
