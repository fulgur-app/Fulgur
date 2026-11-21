use super::{
    compression::compress_content,
    devices::Device,
    types::{ShareFileRequest, ShareResult},
};
use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token, with_valid_token},
        synchronization::{MAX_HTTP_SMALL_RESPONSE_BYTES, SynchronizationError, handle_ureq_error},
    },
    utils::crypto_helper::{self, is_valid_public_key},
};
use fulgur_common::api::shares::ShareFilePayload;
use sha2::{Digest, Sha256};
use std::sync::Arc;

pub const MAX_SYNC_SHARE_PAYLOAD_BYTES: usize = 1024 * 1024;

/// Maximum number of pending share ids the client will accept in a single server response (`/api/v2/begin`).
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
    // Fail fast on missing credentials before compressing and encrypting
    get_valid_token(profile, token_state, http_agent)?;
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
                    let file_name = request.file_name.clone();
                    let deduplication_hash = deduplication_hash.clone();
                    let compressed_content = compressed_content.as_slice();
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
                            match encrypt_content_for_device(compressed_content, public_key) {
                                Ok(content) => content,
                                Err(e) => {
                                    log::error!(
                                        "Failed to encrypt content for device {device_id}: {e}"
                                    );
                                    return (device_id.clone(), Err(e));
                                }
                            };
                        let result = with_valid_token(profile, token_state, http_agent, |token| {
                            send_share_request(
                                &share_url,
                                token,
                                encrypted_content.clone(),
                                &file_name,
                                &device_id,
                                deduplication_hash.clone(),
                                http_agent,
                            )
                        });
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
mod tests;
