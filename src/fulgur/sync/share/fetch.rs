use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{
            MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES, SynchronizationError, handle_ureq_error,
            max_http_bulk_shares_response_bytes,
        },
    },
};
use fulgur_common::api::shares::SharedFileResponse;
use std::sync::Arc;

/// Atomically fetch and delete all pending shares for the current device on
/// a single profile's server.
///
/// Will be deprecated in 0.11.0.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `server_max_file_size`: The server-advertised max file size
///
/// ### Errors
/// Returns a `SynchronizationError` if the profile has no server URL, the
/// authentication token cannot be obtained, the HTTP request fails, the
/// response exceeds the per-server bulk cap, or the response cannot be
/// deserialized.
///
/// ### Returns
/// - `Ok(Vec<SharedFileResponse>)`: The shares that were drained from the server
/// - `Err(SynchronizationError)`: If the request failed or the response was invalid
pub fn fetch_pending_shares(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    server_max_file_size: u64,
) -> Result<Vec<SharedFileResponse>, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(profile, token_state, http_agent)?;
    let shares_url = format!("{server_url}/api/shares");
    let mut response = http_agent
        .get(&shares_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Failed to fetch pending shares"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(max_http_bulk_shares_response_bytes(server_max_file_size))
        .read_to_string()
        .map_err(|e| {
            log::error!("Failed to read pending shares: {e}");
            SynchronizationError::Other(e.to_string())
        })?;
    let shares: Vec<SharedFileResponse> = serde_json::from_str(&body).map_err(|e| {
        log::error!("Failed to parse pending shares: {e}");
        SynchronizationError::InvalidResponse(e.to_string())
    })?;
    log::debug!("Fetched {} pending share(s) from server", shares.len());
    Ok(shares)
}

/// Fetch a single pending share by ID and remove it from the server.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `id`: The share identifier returned by the v2 begin endpoint
///
/// ### Errors
/// Returns a `SynchronizationError` if the profile has no server URL, the
/// authentication token cannot be obtained, the HTTP request fails, the share
/// is missing, or the response is invalid or too large.
///
/// ### Returns
/// - `Ok(SharedFileResponse)`: The share content
/// - `Err(SynchronizationError)`: If the request failed, the share is gone,
///   or the response was invalid or too large
pub fn fetch_share_by_id(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    id: &str,
) -> Result<SharedFileResponse, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(profile, token_state, http_agent)?;
    let share_url = format!("{server_url}/api/shares/{id}");
    let mut response = http_agent
        .get(&share_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Failed to fetch share by id"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| {
            log::error!("Failed to read share response body for id {id}: {e}");
            SynchronizationError::Other(e.to_string())
        })?;
    serde_json::from_str::<SharedFileResponse>(&body).map_err(|e| {
        log::error!("Failed to parse share response body for id {id}: {e}");
        SynchronizationError::InvalidResponse(e.to_string())
    })
}
