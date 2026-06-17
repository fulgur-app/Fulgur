use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{
            SynchronizationError, handle_ureq_error, max_http_bulk_shares_response_bytes,
            max_http_single_share_response_bytes,
        },
    },
    utils::sanitize::sanitize_filename,
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
/// - `Ok(Vec<SharedFileResponse>)`: The shares that were drained from the
///   server, with each `file_name` sanitized against path traversal and
///   control characters
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
    let mut shares: Vec<SharedFileResponse> = serde_json::from_str(&body).map_err(|e| {
        log::error!("Failed to parse pending shares: {e}");
        SynchronizationError::InvalidResponse(e.to_string())
    })?;
    for share in &mut shares {
        share.file_name = sanitize_filename(&share.file_name);
    }
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
/// - `server_max_file_size`: The server-advertised max file size, or `u64::MAX`,
///   used to size the response body cap
///
/// ### Errors
/// Returns a `SynchronizationError` if the profile has no server URL, the
/// authentication token cannot be obtained, the HTTP request fails, the share
/// is missing, or the response is invalid or too large.
///
/// ### Returns
/// - `Ok(SharedFileResponse)`: The share content, with `file_name` sanitized
///   against path traversal and control characters
/// - `Err(SynchronizationError)`: If the request failed, the share is gone,
///   or the response was invalid or too large
pub fn fetch_share_by_id(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    id: &str,
    server_max_file_size: u64,
) -> Result<SharedFileResponse, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let share_url = format!("{server_url}/api/shares/{id}");
    fetch_share_at_url(
        profile,
        token_state,
        http_agent,
        &share_url,
        id,
        server_max_file_size,
    )
}

/// Fetch a single available share by ID without consuming it (Fulgurant 0.8.0+).
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `id`: The share identifier announced by the doorbell event
/// - `server_max_file_size`: The server-advertised max file size, or `u64::MAX`,
///   used to size the response body cap
///
/// ### Errors
/// - Returns a `SynchronizationError` if the profile has no server URL, the
///   authentication token cannot be obtained, the HTTP request fails, the share
///   is missing, or the response is invalid or too large.
///
/// ### Returns
/// - `Ok(SharedFileResponse)`: The share content, left intact server-side, with
///   `file_name` sanitized against path traversal and control characters
/// - `Err(SynchronizationError)`: If the request failed, the share is gone,
///   or the response was invalid or too large
pub fn fetch_share_by_id_v2(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    id: &str,
    server_max_file_size: u64,
) -> Result<SharedFileResponse, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let share_url = format!("{server_url}/api/v2/shares/{id}");
    fetch_share_at_url(
        profile,
        token_state,
        http_agent,
        &share_url,
        id,
        server_max_file_size,
    )
}

/// Acknowledge a successful download of a v2 share, consuming it server-side (Fulgurant 0.8.0+).
///
/// ### Arguments
/// - `profile`: The server profile to acknowledge against
/// - `token_state`: Per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `id`: The share identifier to acknowledge
///
/// ### Errors
/// - Returns a `SynchronizationError` if the profile has no server URL, the
///   authentication token cannot be obtained, or the HTTP request fails with a
///   status other than 204 or 404.
///
/// ### Returns
/// - `Ok(())`: The share was acknowledged, or was already gone (404)
/// - `Err(SynchronizationError)`: If the request failed
pub fn acknowledge_share_download(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    id: &str,
) -> Result<(), SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let token = get_valid_token(profile, token_state, http_agent)?;
    let ack_url = format!("{server_url}/api/v2/shares/{id}/successful");
    match http_agent
        .post(&ack_url)
        .header("Authorization", &format!("Bearer {token}"))
        .send("")
    {
        Ok(_) => {
            log::debug!("Acknowledged successful download of share {id}");
            Ok(())
        }
        Err(ureq::Error::StatusCode(404)) => {
            log::debug!(
                "Share {id} no longer available to acknowledge (404); treating as already consumed"
            );
            Ok(())
        }
        Err(e) => Err(handle_ureq_error(e, "Failed to acknowledge share download")),
    }
}

/// Fetch a single share from a fully-qualified URL, capping and sanitizing the response. Backs both the
/// v1 ([`fetch_share_by_id`]) and v2 ([`fetch_share_by_id_v2`]) per-id fetches.
///
/// ### Arguments
/// - `profile`: The server profile, used to obtain a valid token
/// - `token_state`: Per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_url`: The fully-qualified share URL to GET
/// - `id`: The share identifier, used only for logging
/// - `server_max_file_size`: The server-advertised max file size, or `u64::MAX`,
///   used to size the response body cap
///
/// ### Errors
/// - Returns a `SynchronizationError` if the authentication token cannot be
///   obtained, the HTTP request fails, or the response is invalid or too large.
///
/// ### Returns
/// - `Ok(SharedFileResponse)`: The share content, with `file_name` sanitized
/// - `Err(SynchronizationError)`: If the request or response was invalid
fn fetch_share_at_url(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    share_url: &str,
    id: &str,
    server_max_file_size: u64,
) -> Result<SharedFileResponse, SynchronizationError> {
    let token = get_valid_token(profile, token_state, http_agent)?;
    let mut response = http_agent
        .get(share_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Failed to fetch share by id"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(max_http_single_share_response_bytes(server_max_file_size))
        .read_to_string()
        .map_err(|e| {
            log::error!("Failed to read share response body for id {id}: {e}");
            SynchronizationError::Other(e.to_string())
        })?;
    let mut share = serde_json::from_str::<SharedFileResponse>(&body).map_err(|e| {
        log::error!("Failed to parse share response body for id {id}: {e}");
        SynchronizationError::InvalidResponse(e.to_string())
    })?;
    share.file_name = sanitize_filename(&share.file_name);
    Ok(share)
}
