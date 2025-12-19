use std::{sync::atomic::Ordering, thread, time::Duration};

use crate::fulgur::{crypto_helper, icons::CustomIcon};
use flate2::Compression;
use flate2::read::{GzDecoder, GzEncoder};
use fulgur_common::api::BeginResponse;
use fulgur_common::api::devices::DeviceResponse;
use gpui_component::Icon;
use serde::Serialize;
use std::io::Read;

pub type Device = DeviceResponse;

/// Compress content using gzip compression
///
/// @param content: The content to compress
///
/// @return: The compressed content as bytes
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
/// @param compressed: The compressed content as bytes
///
/// @return: The decompressed content as string
pub fn decompress_content(compressed: &[u8]) -> anyhow::Result<String> {
    let mut decoder = GzDecoder::new(compressed);
    let mut decompressed = String::new();
    decoder.read_to_string(&mut decompressed)?;
    Ok(decompressed)
}

/// Get the icon for the device
///
/// @param device: The device
///
/// @return: The icon for the device
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
/// @param server_url: The server URL
///
/// @param email: The email
///
/// @param key: The key
///
/// @return: The devices
pub fn get_devices(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
) -> anyhow::Result<Vec<Device>> {
    if server_url.is_none() {
        return Err(anyhow::anyhow!("Server URL is missing"));
    }
    if email.is_none() {
        return Err(anyhow::anyhow!("Email is missing"));
    }
    if key.is_none() {
        return Err(anyhow::anyhow!("Key is missing"));
    }
    let decrypted_key = crypto_helper::decrypt(&key.unwrap()).unwrap();
    let devices_url = format!("{}/api/devices", server_url.unwrap());
    let response = ureq::get(&devices_url)
        .header("Authorization", &format!("Bearer {}", decrypted_key))
        .header("X-User-Email", &email.unwrap())
        .call();
    match response {
        Ok(mut response) => {
            let devices: Vec<Device> = response.body_mut().read_json::<Vec<Device>>()?;
            log::debug!("Retrieved {} devices from server", devices.len());
            Ok(devices)
        }
        Err(e) => Err(anyhow::anyhow!("Failed to get devices: {}", e)),
    }
}

/// Fetch the user's encryption key from the server
///
/// The server manages a shared encryption key per user that all their devices can access
///
/// @param server_url: The server URL
///
/// @param email: The user's email
///
/// @param device_key: The decrypted device authentication key
///
/// @return: The user's encryption key (base64-encoded)
fn fetch_encryption_key(server_url: &str, email: &str, device_key: &str) -> anyhow::Result<String> {
    let key_url = format!("{}/api/encryption-key", server_url);
    let mut response = ureq::get(&key_url)
        .header("Authorization", &format!("Bearer {}", device_key))
        .header("X-User-Email", email)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to fetch encryption key: {}", e))?;
    let body = response.body_mut().read_to_string()?;
    let json: serde_json::Value = serde_json::from_str(&body)?;
    let encryption_key = json["encryption_key"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("Invalid response: missing encryption_key"))?;
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
/// @param server_url: The server URL
///
/// @param email: The email
///
/// @param key: The encrypted device authentication key
///
/// @param payload: The payload to share the file with (content will be encrypted)
///
/// @return: The expiration date of the shared file
pub fn share_file(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
    payload: ShareFilePayload,
) -> anyhow::Result<String> {
    if server_url.is_none() {
        return Err(anyhow::anyhow!("Server URL is missing"));
    }
    if email.is_none() {
        return Err(anyhow::anyhow!("Email is missing"));
    }
    if key.is_none() {
        return Err(anyhow::anyhow!("Key is missing"));
    }
    if payload.content.is_empty() {
        return Err(anyhow::anyhow!("Content is missing"));
    }
    if payload.content.len() > 1024 * 1024 {
        // 1MB
        return Err(anyhow::anyhow!("Content is too large to share"));
    }
    if payload.file_name.is_empty() {
        return Err(anyhow::anyhow!("File name is missing"));
    }
    if payload.device_ids.is_empty() {
        return Err(anyhow::anyhow!("Device IDs are missing"));
    }
    let server_url_str = server_url.as_ref().unwrap();
    let email_str = email.as_ref().unwrap();
    let decrypted_device_key = crypto_helper::decrypt(&key.unwrap())?;
    let encryption_key = fetch_encryption_key(server_url_str, email_str, &decrypted_device_key)?;
    let compressed_content = compress_content(&payload.content)?;
    let encrypted_content = crypto_helper::encrypt_bytes(&compressed_content, &encryption_key)?;
    let encrypted_payload = ShareFilePayload {
        content: encrypted_content,
        file_name: payload.file_name.clone(),
        device_ids: payload.device_ids,
    };
    let share_url = format!("{}/api/share", server_url_str);
    let mut response = ureq::post(&share_url)
        .header("Authorization", &format!("Bearer {}", decrypted_device_key))
        .header("X-User-Email", email_str)
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload)?;
    if response.status() == 200 {
        let body = response.body_mut().read_to_string()?;
        let json: serde_json::Value = serde_json::from_str(&body)?;
        let expiration_date = json["expiration_date"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("Invalid response: missing expiration_date"))?;
        log::info!(
            "File {} shared successfully until {}",
            payload.file_name,
            expiration_date
        );
        Ok(expiration_date.to_string())
    } else {
        Err(anyhow::anyhow!(
            "Failed to share file: {}",
            response.body_mut().read_to_string()?
        ))
    }
}

/// Initial synchronization with the server
///
/// This endpoint returns both the encryption key and any shared files waiting for this device
///
/// @param server_url: The server URL
///
/// @param email: The email
///
/// @param key: The encrypted device authentication key
///
/// @return: The begin response containing encryption key and shared files
pub fn initial_synchronization(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
) -> anyhow::Result<BeginResponse> {
    if server_url.is_none() {
        return Err(anyhow::anyhow!("Server URL is missing"));
    }
    if email.is_none() {
        return Err(anyhow::anyhow!("Email is missing"));
    }
    if key.is_none() {
        return Err(anyhow::anyhow!("Key is missing"));
    }
    let server_url_str = server_url.as_ref().unwrap();
    let email_str = email.as_ref().unwrap();
    let decrypted_device_key = crypto_helper::decrypt(&key.unwrap())?;
    let begin_url = format!("{}/api/begin", server_url_str);
    let mut response = ureq::get(&begin_url)
        .header("Authorization", &format!("Bearer {}", decrypted_device_key))
        .header("X-User-Email", email_str)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to fetch shared files: {}", e))?;
    let body = response.body_mut().read_to_string()?;
    let begin_response: BeginResponse = serde_json::from_str(&body)?;
    log::info!(
        "Initial synchronization successful with {} shared files",
        begin_response.shares.len()
    );
    Ok(begin_response)
}

/// Fetches shared files from the server and stores them for processing without blocking app startup
///
/// @param entity: The Fulgur entity
///
/// @param cx: The application context
///
/// @return: The begin response from the server containing encryption key, device name, and shared files
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
                log::error!("Failed to fetch shared files: {}", e);
                is_connected.store(false, Ordering::Relaxed);
            }
        }
    });
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
