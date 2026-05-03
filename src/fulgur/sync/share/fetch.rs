use crate::fulgur::{
    settings::SynchronizationSettings,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{SynchronizationError, handle_ureq_error},
    },
};
use fulgur_common::api::shares::SharedFileResponse;
use std::sync::Arc;

/// Atomically fetch and delete all pending shares for the current device.
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `token_state`: Arc to the token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(Vec<SharedFileResponse>)`: The shares that were drained from the server
/// - `Err(SynchronizationError)`: If the request failed or the response was invalid
pub fn fetch_pending_shares(
    synchronization_settings: &SynchronizationSettings,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<Vec<SharedFileResponse>, SynchronizationError> {
    let Some(server_url) = synchronization_settings.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(synchronization_settings, token_state, http_agent)?;
    let shares_url = format!("{server_url}/api/shares");
    let mut response = http_agent
        .get(&shares_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Failed to fetch pending shares"))?;
    let shares: Vec<SharedFileResponse> = response
        .body_mut()
        .read_json::<Vec<SharedFileResponse>>()
        .map_err(|e| {
            log::error!("Failed to read pending shares: {e}");
            SynchronizationError::InvalidResponse(e.to_string())
        })?;
    log::debug!("Fetched {} pending share(s) from server", shares.len());
    Ok(shares)
}
