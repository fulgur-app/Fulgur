use super::error::{SynchronizationError, handle_ureq_error};
use super::limits::{MAX_HTTP_SMALL_RESPONSE_BYTES, resolve_server_max_file_size};
use super::version::{
    FULGURANT_VERSION_HEADER, MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY, server_meets_minimum_version,
};
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::sync::access_token::{TokenStateManager, get_valid_token};
use crate::fulgur::sync::share;
use fulgur_common::api::shares::SharedFileResponse;
use fulgur_common::api::sync::{BeginV2Response, InitialSynchronizationPayload};
use parking_lot::Mutex;
use std::collections::HashSet;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Device name, size limit, and shares retrieved by an initial synchronization.
pub struct BeginOutcome {
    /// The name the server has registered for this device.
    pub device_name: String,
    /// The shares announced by the begin response that were successfully fetched.
    pub shares: Vec<SharedFileResponse>,
    /// Server-advertised maximum file size, or `None` when the server sets no limit.
    pub max_file_size_bytes: Option<u64>,
}

/// Outcome of an initial synchronization.
pub struct InitialSyncOutcome {
    /// Device name, max file size, and successfully fetched pending shares.
    pub begin: BeginOutcome,
    /// The `min_fulgur_version` advertised by the begin response, if any.
    pub min_fulgur_version: Option<String>,
    /// Raw `x-fulgurant-version` header advertised by the server.
    pub fulgurant_version: Option<String>,
}

/// Parsed `POST /api/v2/begin` response together with the advertised server version.
struct BeginResponseOutcome {
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
/// Returns a `SynchronizationError` if the request fails, the server is older
/// than the minimum supported Fulgurant version, the response cannot be read or
/// parsed, or the server announces more pending shares than the client allows.
///
/// ### Returns
/// - `Ok(BeginResponseOutcome)`: The parsed begin response and advertised version
/// - `Err(SynchronizationError)`: If the begin call failed or returned an invalid response
fn perform_begin(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<BeginResponseOutcome, SynchronizationError> {
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
        .map_err(|e| handle_ureq_error(e, "Failed to begin synchronization"))?;
    let version_header = response
        .headers()
        .get(FULGURANT_VERSION_HEADER)
        .and_then(|value| value.to_str().ok())
        .map(str::to_owned);
    if !server_meets_minimum_version(version_header.as_deref()) {
        log::error!(
            "Server advertises Fulgurant version {}, which is below the required {}",
            version_header.as_deref().unwrap_or("<none>"),
            MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY
        );
        return Err(SynchronizationError::ServerTooOld {
            required: MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY,
        });
    }
    let body = match response
        .body_mut()
        .with_config()
        .limit(MAX_HTTP_SMALL_RESPONSE_BYTES)
        .read_to_string()
    {
        Ok(body) => body,
        Err(e) => {
            log::error!("Failed to read begin response body: {e}");
            return Err(SynchronizationError::Other(e.to_string()));
        }
    };
    let begin: BeginV2Response = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse begin response body: {e}");
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    if begin.share_ids.len() > share::MAX_PENDING_SHARES_PER_RESPONSE {
        log::error!(
            "Server returned {} pending share ids, exceeding the client limit of {}",
            begin.share_ids.len(),
            share::MAX_PENDING_SHARES_PER_RESPONSE
        );
        return Err(SynchronizationError::InvalidResponse(format!(
            "Server returned too many pending share ids ({} > {})",
            begin.share_ids.len(),
            share::MAX_PENDING_SHARES_PER_RESPONSE
        )));
    }
    Ok(BeginResponseOutcome {
        response: begin,
        version_header,
    })
}

/// List the IDs of the device's pending shares without consuming them.
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
pub fn list_pending_share_ids(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<Vec<String>, SynchronizationError> {
    Ok(perform_begin(profile, token_state, http_agent)?
        .response
        .share_ids)
}

/// Maximum number of concurrent share fetch worker threads.
const MAX_FETCH_WORKERS: usize = 8;

/// Fetch each announced share by id in parallel.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_ids`: The announced pending share ids to retrieve
/// - `server_max_file_size`: Server-advertised max file size used to bound each response
///
/// ### Returns
/// - `Vec<SharedFileResponse>`: The successfully fetched shares; failures are logged and skipped
fn fetch_shares_for_ids(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    share_ids: &[String],
    server_max_file_size: u64,
) -> Vec<SharedFileResponse> {
    if share_ids.is_empty() {
        return Vec::new();
    }
    let worker_count = MAX_FETCH_WORKERS.min(share_ids.len());
    let next_index = AtomicUsize::new(0);
    let results = Mutex::new(Vec::with_capacity(share_ids.len()));
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    let Some(id) = share_ids.get(index) else {
                        break;
                    };
                    match share::fetch_share_by_id(
                        profile,
                        token_state,
                        http_agent,
                        id,
                        server_max_file_size,
                    ) {
                        Ok(s) => results.lock().push(s),
                        Err(e) => log::warn!("Skipping share id {id}: {e}"),
                    }
                }
            });
        }
    });
    results.into_inner()
}

/// Initial synchronization with the server.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `pending_ack_share_ids`: Ack set the fetched ids are registered into
///
/// ### Errors
/// - Returns a `SynchronizationError` if the begin request fails (network failure,
///   authentication failure, server older than the minimum supported Fulgurant
///   version, or invalid response).
///
/// ### Returns
/// - `Ok(InitialSyncOutcome)`: Begin response plus the server's advertised
///   minimum supported Fulgur version
/// - `Err(SynchronizationError)`: If the begin call failed
#[allow(clippy::implicit_hasher)]
pub fn initial_synchronization(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
    pending_ack_share_ids: &Arc<Mutex<HashSet<String>>>,
) -> Result<InitialSyncOutcome, SynchronizationError> {
    let BeginResponseOutcome {
        response: begin,
        version_header,
    } = perform_begin(profile, token_state, http_agent)?;
    let min_fulgur_version = begin.min_fulgur_version.clone();
    let server_max_file_size = resolve_server_max_file_size(begin.max_file_size_bytes);
    let shares = fetch_shares_for_ids(
        profile,
        token_state,
        http_agent,
        &begin.share_ids,
        server_max_file_size,
    );
    {
        let mut ack_set = pending_ack_share_ids.lock();
        for share in &shares {
            ack_set.insert(share.id.clone());
        }
    }
    log::info!(
        "Initial synchronization successful: {} announced, {} retrieved",
        begin.share_ids.len(),
        shares.len()
    );
    Ok(InitialSyncOutcome {
        begin: BeginOutcome {
            device_name: begin.device_name,
            shares,
            max_file_size_bytes: begin.max_file_size_bytes,
        },
        min_fulgur_version,
        fulgurant_version: version_header,
    })
}
