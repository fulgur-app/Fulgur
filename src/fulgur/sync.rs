use crate::fulgur::{crypto_helper, icons::CustomIcon};
use gpui_component::Icon;
use serde::{Deserialize, Serialize};

// Test the synchronization connection
// @param server_url: The server URL
// @param email: The email
// @param key: The key
// @return: The result of the connection test
pub fn test_synchronization_connection(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
) -> SynchronizationTestResult {
    if server_url.is_none() {
        return SynchronizationTestResult::Failure("Server URL is missing".to_string());
    }
    if email.is_none() {
        return SynchronizationTestResult::Failure("Email is missing".to_string());
    }
    if key.is_none() {
        return SynchronizationTestResult::Failure("Key is missing".to_string());
    }
    let decrypted_key = crypto_helper::decrypt(&key.unwrap()).unwrap();
    let ping_url = format!("{}/api/ping", server_url.unwrap());
    let response = ureq::get(&ping_url)
        .header("Authorization", &format!("Bearer {}", decrypted_key))
        .header("X-User-Email", &email.unwrap())
        .call();
    if response.is_ok() {
        return SynchronizationTestResult::Success;
    } else {
        log::error!("Connection test failed: {}", response.unwrap_err());
        return SynchronizationTestResult::Failure("Connection test failed".to_string());
    }
}

#[derive(Clone, PartialEq)]
pub enum SynchronizationTestResult {
    Success,
    Failure(String),
}

#[derive(Debug, Deserialize, Clone)]
#[allow(dead_code)]
pub struct Device {
    pub id: String,
    pub name: String,
    pub device_type: String,
    pub created_at: String,
    pub expires_at: String,
}

// Get the icon for the device
// @param device: The device
// @return: The icon for the device
pub fn get_icon(device: &Device) -> Icon {
    match device.device_type.to_lowercase().as_str() {
        "desktop" => Icon::new(CustomIcon::Computer),
        "laptop" => Icon::new(CustomIcon::Laptop),
        "server" => Icon::new(CustomIcon::Server),
        _ => Icon::new(CustomIcon::Computer),
    }
}

// Get the devices from the server
// @param server_url: The server URL
// @param email: The email
// @param key: The key
// @return: The devices
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
/// The server manages a shared encryption key per user that all their devices can access
/// @param server_url: The server URL
/// @param email: The user's email
/// @param device_key: The decrypted device authentication key
/// @return: The user's encryption key (base64-encoded)
fn fetch_encryption_key(server_url: &str, email: &str, device_key: &str) -> anyhow::Result<String> {
    let key_url = format!("{}/api/encryption-key", server_url);
    let mut response = ureq::get(&key_url)
        .header("Authorization", &format!("Bearer {}", device_key))
        .header("X-User-Email", email)
        .call()
        .map_err(|e| anyhow::anyhow!("Failed to fetch encryption key: {}", e))?;

    let body = response.body_mut().read_to_string()?;

    // Parse JSON response: {"encryption_key": "base64_key"}
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

// Share the file with the devices
// @param server_url: The server URL
// @param email: The email
// @param key: The encrypted device authentication key
// @param payload: The payload to share the file with (content will be encrypted)
// @return: The result of the sharing
pub fn share_file(
    server_url: Option<String>,
    email: Option<String>,
    key: Option<String>,
    payload: ShareFilePayload,
) -> anyhow::Result<()> {
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
    let encrypted_content = crypto_helper::encrypt_content(&payload.content, &encryption_key)?;
    log::debug!(
        "Encrypted {} bytes to {} bytes",
        payload.content.len(),
        encrypted_content.len()
    );

    // Create payload with encrypted content
    let encrypted_payload = ShareFilePayload {
        content: encrypted_content,
        file_name: payload.file_name,
        device_ids: payload.device_ids,
    };

    // Send the encrypted content to the server
    let share_url = format!("{}/api/share", server_url_str);
    let mut response = ureq::post(&share_url)
        .header("Authorization", &format!("Bearer {}", decrypted_device_key))
        .header("X-User-Email", email_str)
        .header("Content-Type", "application/json")
        .send_json(encrypted_payload)?;

    if response.status() == 200 {
        log::info!("File shared successfully with end-to-end encryption");
        Ok(())
    } else {
        Err(anyhow::anyhow!(
            "Failed to share file: {}",
            response.body_mut().read_to_string()?
        ))
    }
}
