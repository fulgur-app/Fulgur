use crate::fulgur::{crypto_helper, icons::CustomIcon};
use gpui_component::Icon;
use serde::Deserialize;

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

impl Device {
    // Get the icon for the device
    // @return: The icon for the device
    pub fn get_icon(&self) -> Icon {
        match self.device_type.to_lowercase().as_str() {
            "desktop" => Icon::new(CustomIcon::Computer),
            "laptop" => Icon::new(CustomIcon::Laptop),
            "server" => Icon::new(CustomIcon::Server),
            _ => Icon::new(CustomIcon::Computer),
        }
    }
}

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
