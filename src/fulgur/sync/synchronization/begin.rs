use super::error::{SynchronizationError, handle_ureq_error};
use super::limits::{
    MAX_HTTP_SMALL_RESPONSE_BYTES, MAX_HTTP_V1_BEGIN_RESPONSE_BYTES, resolve_server_max_file_size,
};
use super::version::{FULGURANT_VERSION_HEADER, version_supports_v2_share_flow};
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::sync::access_token::{TokenStateManager, get_valid_token};
use crate::fulgur::sync::share;
use crate::fulgur::utils::sanitize::sanitize_filename;
use fulgur_common::api::shares::SharedFileResponse;
use fulgur_common::api::sync::{BeginResponse, BeginV2Response, InitialSynchronizationPayload};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;

/// Initial synchronization with the server.
///
/// ### Description
/// Attempts the v2 begin flow first (`POST /api/v2/begin` + per-share fetch).
/// If the server returns HTTP 404 (endpoint not deployed yet), falls back to
/// the legacy v1 flow (`POST /api/begin`) for compatibility with Fulgurant
/// servers that have not been upgraded.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `pending_ack_share_ids`: Ack set the v2 read/ack flow registers fetched ids
///   into so the decryption pass acknowledges them after a successful download
///
/// ### Errors
/// Returns a `SynchronizationError` if both the v2 and the legacy v1 begin
/// requests fail (network failure, authentication failure, or invalid response).
///
/// ### Returns
/// - `Ok(InitialSyncOutcome)`: Begin response plus the server's advertised
///   minimum supported Fulgur version (v2 only)
/// - `Err(SynchronizationError)`: If both v2 and v1 begin calls failed
// The ack set is always the default-hasher `HashSet<String>` owned by `SyncState`;
// generalizing over `BuildHasher` would only add noise.
#[allow(clippy::implicit_hasher)]
pub fn initial_synchronization(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    pending_ack_share_ids: &Arc<Mutex<HashSet<String>>>,
) -> Result<InitialSyncOutcome, SynchronizationError> {
    match initial_synchronization_v2(profile, token_state, http_agent, pending_ack_share_ids) {
        Ok(response) => Ok(response),
        Err(SynchronizationError::ServerError(404)) => {
            log::warn!(
                "Server does not support /api/v2/begin (404); falling back to legacy /api/begin"
            );
            initial_synchronization_v1(profile, token_state, http_agent)
        }
        Err(e) => Err(e),
    }
}

/// Outcome of an initial synchronization.
pub struct InitialSyncOutcome {
    /// Device name, max file size, and successfully fetched pending shares.
    pub begin: BeginResponse,
    /// The `min_fulgur_version` advertised by the v2 begin response, if any.
    /// `None` for legacy v1 servers, which do not advertise it.
    pub min_fulgur_version: Option<String>,
    /// Raw `x-fulgurant-version` header advertised by the server, if any.
    /// `None` for Fulgurant before 0.7.0, which does not advertise it.
    pub fulgurant_version: Option<String>,
}

/// Parsed `POST /api/v2/begin` response together with the advertised server version.
struct BeginV2Outcome {
    /// The decoded begin response (device name, pending share ids, max file size).
    response: BeginV2Response,
    /// Raw `x-fulgurant-version` header value, if the server advertised one.
    version_header: Option<String>,
}

/// Perform the `POST /api/v2/begin` request and parse its response.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Errors
/// Returns a `SynchronizationError` if the request fails, the response cannot be
/// read or parsed, or the server announces more pending shares than the client allows.
///
/// ### Returns
/// - `Ok(BeginV2Outcome)`: The parsed begin response and advertised version
/// - `Err(SynchronizationError)`: If the begin call failed or returned an invalid response
fn perform_begin_v2(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<BeginV2Outcome, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let Some(public_key) = profile.public_key.clone() else {
        return Err(SynchronizationError::MissingEncryptionKey);
    };
    let token = get_valid_token(profile, token_state, http_agent)?;
    let begin_url = format!("{server_url}/api/v2/begin");
    let payload = InitialSynchronizationPayload { public_key };
    let mut response = http_agent
        .post(begin_url)
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(payload)
        .map_err(|e| handle_ureq_error(e, "Failed to begin synchronization (v2)"))?;
    let version_header = response
        .headers()
        .get(FULGURANT_VERSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = match response
        .body_mut()
        .with_config()
        .limit(MAX_HTTP_SMALL_RESPONSE_BYTES)
        .read_to_string()
    {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read v2 begin response body: {e}");
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let begin_v2: BeginV2Response = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse v2 begin response body: {e}");
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    if begin_v2.share_ids.len() > share::MAX_PENDING_SHARES_PER_RESPONSE {
        log::error!(
            "Server returned {} pending share ids, exceeding the client limit of {}",
            begin_v2.share_ids.len(),
            share::MAX_PENDING_SHARES_PER_RESPONSE
        );
        return Err(SynchronizationError::InvalidResponse(format!(
            "Server returned too many pending share ids ({} > {})",
            begin_v2.share_ids.len(),
            share::MAX_PENDING_SHARES_PER_RESPONSE
        )));
    }
    Ok(BeginV2Outcome {
        response: begin_v2,
        version_header,
    })
}

/// List the IDs of the device's pending shares via `POST /api/v2/begin` without consuming them.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Errors
/// Returns a `SynchronizationError` if the begin call fails or returns an invalid response.
///
/// ### Returns
/// - `Ok(Vec<String>)`: The IDs of the device's pending shares
/// - `Err(SynchronizationError)`: If the begin call failed
pub fn list_pending_share_ids_v2(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<Vec<String>, SynchronizationError> {
    Ok(perform_begin_v2(profile, token_state, http_agent)?
        .response
        .share_ids)
}

/// Fetch each announced share by id in parallel, choosing the retrieval endpoint
/// based on whether the server supports the v2 read/ack flow.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_ids`: The announced pending share ids to retrieve
/// - `server_max_file_size`: Server-advertised max file size used to bound each response
/// - `use_v2_flow`: When `true`, fetch via the non-consuming `GET /api/v2/shares/:id`;
///   otherwise via the consuming `GET /api/shares/:id`
///
/// ### Returns
/// - `Vec<SharedFileResponse>`: The successfully fetched shares; failures are logged and skipped
fn fetch_shares_for_ids(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    share_ids: &[String],
    server_max_file_size: u64,
    use_v2_flow: bool,
) -> Vec<SharedFileResponse> {
    std::thread::scope(|scope| {
        let handles: Vec<_> = share_ids
            .iter()
            .map(|id| {
                scope.spawn(move || {
                    let result = if use_v2_flow {
                        share::fetch_share_by_id_v2(
                            profile,
                            token_state,
                            http_agent,
                            id,
                            server_max_file_size,
                        )
                    } else {
                        share::fetch_share_by_id(
                            profile,
                            token_state,
                            http_agent,
                            id,
                            server_max_file_size,
                        )
                    };
                    (id.as_str(), result)
                })
            })
            .collect();
        handles
            .into_iter()
            .filter_map(|h| match h.join() {
                Ok((_, Ok(s))) => Some(s),
                Ok((id, Err(e))) => {
                    log::warn!("Skipping share id {id}: {e}");
                    None
                }
                Err(_) => {
                    log::error!("Fetch share worker thread panicked");
                    None
                }
            })
            .collect()
    })
}

/// Initial synchronization via the v2 begin flow.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `pending_ack_share_ids`: Ack set the v2 read/ack flow registers fetched ids into
///
/// ### Returns
/// - `Ok(InitialSyncOutcome)`: Begin response and advertised minimum Fulgur version
/// - `Err(SynchronizationError)`: If the v2 begin call failed or returned an invalid response
fn initial_synchronization_v2(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    pending_ack_share_ids: &Arc<Mutex<HashSet<String>>>,
) -> Result<InitialSyncOutcome, SynchronizationError> {
    let BeginV2Outcome {
        response: begin_v2,
        version_header,
    } = perform_begin_v2(profile, token_state, http_agent)?;
    let min_fulgur_version = begin_v2.min_fulgur_version.clone();
    let use_v2_flow = version_supports_v2_share_flow(version_header.as_deref());
    let server_max_file_size = resolve_server_max_file_size(begin_v2.max_file_size_bytes);
    let shares = fetch_shares_for_ids(
        profile,
        token_state,
        http_agent,
        &begin_v2.share_ids,
        server_max_file_size,
        use_v2_flow,
    );
    if use_v2_flow {
        let mut ack_set = pending_ack_share_ids.lock();
        for share in &shares {
            ack_set.insert(share.id.clone());
        }
    }
    log::info!(
        "Initial synchronization (v2) successful: {} announced, {} retrieved (read/ack flow: {use_v2_flow})",
        begin_v2.share_ids.len(),
        shares.len()
    );
    Ok(InitialSyncOutcome {
        begin: BeginResponse {
            device_name: begin_v2.device_name,
            shares,
            max_file_size_bytes: begin_v2.max_file_size_bytes,
        },
        min_fulgur_version,
        fulgurant_version: version_header,
    })
}

/// Initial synchronization via the legacy v1 begin flow.
///
/// ### Description
/// Calls `POST /api/begin`, which returns the device name, max file size and
/// pending shares inline. Used as a fallback when the server does not yet
/// expose `/api/v2/begin`.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(InitialSyncOutcome)`: Begin response with no advertised minimum Fulgur
///   version (legacy v1 servers do not advertise one)
/// - `Err(SynchronizationError)`: If the v1 begin call failed or returned an invalid response
fn initial_synchronization_v1(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<InitialSyncOutcome, SynchronizationError> {
    let Some(server_url) = profile.server_url.clone() else {
        return Err(SynchronizationError::ServerUrlMissing);
    };
    let Some(public_key) = profile.public_key.clone() else {
        return Err(SynchronizationError::MissingEncryptionKey);
    };
    let token = get_valid_token(profile, token_state, http_agent)?;
    let begin_url = format!("{server_url}/api/begin");
    let payload = InitialSynchronizationPayload { public_key };
    let mut response = http_agent
        .post(begin_url)
        .header("Authorization", &format!("Bearer {token}"))
        .send_json(payload)
        .map_err(|e| handle_ureq_error(e, "Failed to begin synchronization (v1)"))?;
    let version_header = response
        .headers()
        .get(FULGURANT_VERSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    let body = match response
        .body_mut()
        .with_config()
        .limit(MAX_HTTP_V1_BEGIN_RESPONSE_BYTES)
        .read_to_string()
    {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read v1 begin response body: {e}");
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let mut begin_response: BeginResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse v1 begin response body: {e}");
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    for share in &mut begin_response.shares {
        share.file_name = sanitize_filename(&share.file_name);
    }
    log::info!(
        "Initial synchronization (v1) successful with {} shared files",
        begin_response.shares.len()
    );
    Ok(InitialSyncOutcome {
        begin: begin_response,
        min_fulgur_version: None,
        fulgurant_version: version_header,
    })
}
