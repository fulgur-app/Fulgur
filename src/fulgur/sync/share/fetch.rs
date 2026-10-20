use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{
            SynchronizationError, handle_ureq_error, max_http_single_share_response_bytes,
        },
    },
    utils::sanitize::sanitize_filename,
};
use fulgur_common::api::shares::SharedFileResponse;
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;

/// Outcome of fetching a share through the profile's claim set.
pub enum ClaimedFetch {
    /// The id was already claimed: the share is queued, being decrypted,
    /// awaiting its acknowledgement, or quarantined. Nothing was downloaded.
    AlreadyClaimed,
    /// The share was downloaded and its id stays claimed.
    Fetched(SharedFileResponse),
    /// The download failed and the claim was released so a later pass can retry.
    Failed(SynchronizationError),
}

/// Claim a share id, then download the share unless it was already claimed.
///
/// ### Arguments
/// - `claimed_ids`: The profile's set of claimed share ids
/// - `share_id`: The share id to claim and fetch
/// - `fetch`: Downloads the share once the id is claimed
///
/// ### Returns
/// - `ClaimedFetch::AlreadyClaimed`: The id was claimed before; `fetch` was not called
/// - `ClaimedFetch::Fetched`: The downloaded share, its id left claimed
/// - `ClaimedFetch::Failed`: The download error, the claim released
#[allow(clippy::implicit_hasher)]
pub fn fetch_unclaimed_share(
    claimed_ids: &Mutex<HashSet<String>>,
    share_id: &str,
    fetch: impl FnOnce() -> Result<SharedFileResponse, SynchronizationError>,
) -> ClaimedFetch {
    if !claimed_ids.lock().insert(share_id.to_owned()) {
        return ClaimedFetch::AlreadyClaimed;
    }
    match fetch() {
        Ok(share) => ClaimedFetch::Fetched(share),
        Err(e) => {
            claimed_ids.lock().remove(share_id);
            ClaimedFetch::Failed(e)
        }
    }
}

/// Fetch a single available share by ID without consuming it.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `id`: The share identifier announced by the begin response or a doorbell event
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
    let share_url = format!("{server_url}/api/v2/shares/{id}");
    let token = get_valid_token(profile, token_state, http_agent)?;
    let mut response = http_agent
        .get(&share_url)
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

/// Acknowledge a successful download of a share, consuming it server-side.
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::Cell;

    fn share(id: &str) -> SharedFileResponse {
        SharedFileResponse {
            id: id.to_string(),
            source_device_id: "device".to_string(),
            file_name: format!("{id}.txt"),
            file_size: 0,
            content: String::new(),
            created_at: String::new(),
            expires_at: String::new(),
            source_device_name: None,
        }
    }

    #[test]
    fn an_already_claimed_share_is_not_downloaded_again() {
        let claimed = Mutex::new(HashSet::from(["a".to_string()]));
        let fetch_calls = Cell::new(0);
        let outcome = fetch_unclaimed_share(&claimed, "a", || {
            fetch_calls.set(fetch_calls.get() + 1);
            Ok(share("a"))
        });
        assert!(matches!(outcome, ClaimedFetch::AlreadyClaimed));
        assert_eq!(fetch_calls.get(), 0);
    }

    #[test]
    fn a_downloaded_share_stays_claimed() {
        let claimed = Mutex::new(HashSet::new());
        let outcome = fetch_unclaimed_share(&claimed, "a", || Ok(share("a")));
        assert!(matches!(outcome, ClaimedFetch::Fetched(ref s) if s.id == "a"));
        assert!(claimed.lock().contains("a"));
    }

    #[test]
    fn a_failed_download_releases_the_claim_for_a_later_pass() {
        let claimed = Mutex::new(HashSet::new());
        let outcome = fetch_unclaimed_share(&claimed, "a", || {
            Err(SynchronizationError::Other("network down".to_string()))
        });
        assert!(matches!(outcome, ClaimedFetch::Failed(_)));
        assert!(!claimed.lock().contains("a"));
    }
}
