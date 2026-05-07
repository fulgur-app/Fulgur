use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{SynchronizationError, handle_ureq_error},
    },
    ui::icons::CustomIcon,
    utils::crypto_helper::is_valid_public_key,
};
use fulgur_common::api::devices::{DeviceResponse, DevicesResponse};
use gpui_component::Icon;
use std::sync::Arc;

pub type Device = DeviceResponse;

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
/// - `profile`: The server profile to query
/// - `token_state`: Arc to the per-profile token state manager (thread-safe with condition variable)
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok((Vec<Device>, Option<u64>))`: The devices and the server-reported maximum share file size (if advertised)
/// - `Err(SynchronizationError)`: If the devices could not be retrieved
pub fn get_devices(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<(Vec<Device>, Option<u64>), SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(profile, token_state, http_agent)?;
    let devices_url = format!("{server_url}/api/devices");
    let mut response = http_agent
        .get(&devices_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Failed to get devices"))?;

    let devices_response: DevicesResponse = response
        .body_mut()
        .read_json::<DevicesResponse>()
        .map_err(|e| {
            log::error!("Failed to read devices: {e}");
            SynchronizationError::InvalidResponse(e.to_string())
        })?;

    let devices = devices_response
        .devices
        .into_iter()
        .map(|mut device| {
            if let Some(ref key) = device.public_key
                && !is_valid_public_key(key)
            {
                log::warn!(
                    "Device '{}' has a malformed public key; ignoring it",
                    device.name
                );
                device.public_key = None;
            }
            device
        })
        .collect::<Vec<_>>();
    log::debug!("Retrieved {} devices from server", devices.len());
    Ok((devices, devices_response.max_file_size_bytes))
}

#[cfg(test)]
mod tests {
    use super::get_devices;
    use crate::fulgur::settings::ServerProfile;
    use crate::fulgur::sync::{
        access_token::TokenStateManager, synchronization::SynchronizationError,
    };
    use std::sync::Arc;

    fn make_http_agent() -> ureq::Agent {
        ureq::Agent::new_with_config(ureq::config::Config::builder().build())
    }

    #[test]
    fn test_get_devices_fails_without_server_url() {
        let profile = ServerProfile::new("Test"); // server_url = None
        let result = get_devices(
            &profile,
            &Arc::new(TokenStateManager::new()),
            &make_http_agent(),
        );
        assert!(
            matches!(result, Err(SynchronizationError::ServerUrlMissing)),
            "Expected ServerUrlMissing, got: {:?}",
            result.err()
        );
    }
}
