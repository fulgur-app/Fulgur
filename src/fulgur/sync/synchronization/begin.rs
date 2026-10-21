use super::error::{SynchronizationError, handle_ureq_error};
use super::limits::{MAX_HTTP_SMALL_RESPONSE_BYTES, resolve_server_max_file_size};
use super::version::{
    FULGURANT_VERSION_HEADER, MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY, server_meets_minimum_version,
};
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::sync::access_token::{TokenStateManager, get_valid_token};
use crate::fulgur::sync::share::{self, ClaimedFetch};
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
    /// Download errors of announced shares rejected for a permanent failure;
    /// their ids stay claimed so they are reported once and never fetched again.
    pub rejected_shares: Vec<SynchronizationError>,
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

/// Queue freshly fetched shares for decryption, skipping ones already pending.
///
/// ### Arguments
/// - `pending_shared_files`: The profile's queue of shares awaiting decryption
/// - `shares`: The shares retrieved by the latest initial synchronization
pub fn queue_pending_shares(
    pending_shared_files: &Mutex<Vec<SharedFileResponse>>,
    shares: Vec<SharedFileResponse>,
) {
    if shares.is_empty() {
        return;
    }
    let mut pending = pending_shared_files.lock();
    let already_queued: HashSet<&str> = pending.iter().map(|share| share.id.as_str()).collect();
    let unseen: Vec<SharedFileResponse> = shares
        .into_iter()
        .filter(|share| !already_queued.contains(share.id.as_str()))
        .collect();
    drop(already_queued);
    pending.extend(unseen);
}

/// Maximum number of concurrent share fetch worker threads.
const MAX_FETCH_WORKERS: usize = 8;

/// Shares downloaded by one pass over the announced share ids.
#[derive(Default)]
struct FetchedShares {
    /// The newly fetched shares.
    shares: Vec<SharedFileResponse>,
    /// Download errors of shares rejected for a permanent failure.
    rejected: Vec<SynchronizationError>,
}

/// Fetch each announced share that is not already claimed, in parallel.
///
/// ### Arguments
/// - `share_ids`: The announced pending share ids to retrieve
/// - `claimed_ids`: The profile's claim set; each id is claimed before its download
/// - `fetch`: Downloads one share by id
///
/// ### Returns
/// - `FetchedShares`: The newly fetched shares and the permanent download
///   errors; claimed ids are skipped and transient failures are logged and skipped
fn fetch_shares_for_ids(
    share_ids: &[String],
    claimed_ids: &Mutex<HashSet<String>>,
    fetch: impl Fn(&str) -> Result<SharedFileResponse, SynchronizationError> + Sync,
) -> FetchedShares {
    if share_ids.is_empty() {
        return FetchedShares::default();
    }
    let worker_count = MAX_FETCH_WORKERS.min(share_ids.len());
    let next_index = AtomicUsize::new(0);
    let results = Mutex::new(FetchedShares {
        shares: Vec::with_capacity(share_ids.len()),
        rejected: Vec::new(),
    });
    std::thread::scope(|scope| {
        for _ in 0..worker_count {
            scope.spawn(|| {
                loop {
                    let index = next_index.fetch_add(1, Ordering::Relaxed);
                    let Some(id) = share_ids.get(index) else {
                        break;
                    };
                    match share::fetch_unclaimed_share(claimed_ids, id, || fetch(id)) {
                        ClaimedFetch::Fetched(s) => results.lock().shares.push(s),
                        ClaimedFetch::AlreadyClaimed => {
                            log::debug!("Skipping share id {id}: already claimed");
                        }
                        ClaimedFetch::Failed(e) => log::warn!("Skipping share id {id}: {e}"),
                        ClaimedFetch::Rejected(e) => {
                            log::warn!("Rejecting share id {id}, not retrying: {e}");
                            results.lock().rejected.push(e);
                        }
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
/// - `pending_ack_share_ids`: The profile's claim set; ids already in it are not fetched
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
    let FetchedShares { shares, rejected } =
        fetch_shares_for_ids(&begin.share_ids, pending_ack_share_ids, |id| {
            share::fetch_share_by_id(profile, token_state, http_agent, id, server_max_file_size)
        });
    log::info!(
        "Initial synchronization successful: {} announced, {} retrieved, {} rejected",
        begin.share_ids.len(),
        shares.len(),
        rejected.len()
    );
    Ok(InitialSyncOutcome {
        begin: BeginOutcome {
            device_name: begin.device_name,
            shares,
            rejected_shares: rejected,
            max_file_size_bytes: begin.max_file_size_bytes,
        },
        min_fulgur_version,
        fulgurant_version: version_header,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

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

    fn queued_ids(pending: &Mutex<Vec<SharedFileResponse>>) -> Vec<String> {
        pending.lock().iter().map(|s| s.id.clone()).collect()
    }

    #[test]
    fn queue_pending_shares_fills_an_empty_queue() {
        let pending = Mutex::new(Vec::new());
        queue_pending_shares(&pending, vec![share("a"), share("b")]);
        assert_eq!(queued_ids(&pending), vec!["a", "b"]);
    }

    #[test]
    fn queue_pending_shares_keeps_shares_the_server_no_longer_announces() {
        let pending = Mutex::new(vec![share("a")]);
        queue_pending_shares(&pending, vec![share("b")]);
        assert_eq!(queued_ids(&pending), vec!["a", "b"]);
    }

    #[test]
    fn queue_pending_shares_does_not_duplicate_already_queued_shares() {
        let pending = Mutex::new(vec![share("a"), share("b")]);
        queue_pending_shares(&pending, vec![share("b"), share("c")]);
        assert_eq!(queued_ids(&pending), vec!["a", "b", "c"]);
    }

    #[test]
    fn queue_pending_shares_leaves_the_queue_untouched_when_nothing_was_fetched() {
        let pending = Mutex::new(vec![share("a")]);
        queue_pending_shares(&pending, Vec::new());
        assert_eq!(queued_ids(&pending), vec!["a"]);
    }

    fn sorted_ids(shares: &[SharedFileResponse]) -> Vec<String> {
        let mut ids: Vec<String> = shares.iter().map(|s| s.id.clone()).collect();
        ids.sort();
        ids
    }

    #[test]
    fn a_restarted_initial_sync_does_not_refetch_shares_it_already_holds() {
        // The server keeps announcing a share until it is acknowledged, so a
        // share whose ack failed, or that was quarantined, comes back on every
        // SSE restart. Fetching it again would open a second tab for it.
        let claimed = Mutex::new(HashSet::new());
        let announced = vec!["a".to_string(), "b".to_string()];
        let fetch_calls = AtomicUsize::new(0);
        let fetch = |id: &str| {
            fetch_calls.fetch_add(1, Ordering::Relaxed);
            Ok(share(id))
        };

        let first_pass = fetch_shares_for_ids(&announced, &claimed, fetch);
        let restarted_pass = fetch_shares_for_ids(&announced, &claimed, fetch);

        assert_eq!(sorted_ids(&first_pass.shares), vec!["a", "b"]);
        assert!(restarted_pass.shares.is_empty());
        assert_eq!(fetch_calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn a_share_whose_fetch_failed_is_fetched_by_the_next_pass() {
        let claimed = Mutex::new(HashSet::new());
        let announced = vec!["a".to_string()];

        let failed_pass = fetch_shares_for_ids(&announced, &claimed, |_| {
            Err(SynchronizationError::Other("network down".to_string()))
        });
        let next_pass = fetch_shares_for_ids(&announced, &claimed, |id| Ok(share(id)));

        assert!(failed_pass.shares.is_empty());
        assert!(failed_pass.rejected.is_empty());
        assert_eq!(sorted_ids(&next_pass.shares), vec!["a"]);
    }

    #[test]
    fn an_oversized_share_is_reported_once_and_skipped_by_later_passes() {
        // An oversized share stays pending server-side and is announced again
        // on every SSE restart; it must not be downloaded again each time.
        let claimed = Mutex::new(HashSet::new());
        let announced = vec!["big".to_string(), "small".to_string()];
        let fetch_calls = AtomicUsize::new(0);
        let fetch = |id: &str| {
            fetch_calls.fetch_add(1, Ordering::Relaxed);
            if id == "big" {
                Err(SynchronizationError::ShareTooLarge { limit_bytes: 1024 })
            } else {
                Ok(share(id))
            }
        };

        let first_pass = fetch_shares_for_ids(&announced, &claimed, fetch);
        let restarted_pass = fetch_shares_for_ids(&announced, &claimed, fetch);

        assert_eq!(sorted_ids(&first_pass.shares), vec!["small"]);
        assert!(matches!(
            first_pass.rejected.as_slice(),
            [SynchronizationError::ShareTooLarge { limit_bytes: 1024 }]
        ));
        assert!(restarted_pass.shares.is_empty());
        assert!(restarted_pass.rejected.is_empty());
        assert_eq!(fetch_calls.load(Ordering::Relaxed), 2);
    }

    #[test]
    fn concurrent_workers_fetch_a_repeated_id_once() {
        let claimed = Mutex::new(HashSet::new());
        let announced = vec!["a".to_string(); MAX_FETCH_WORKERS];
        let fetch_calls = AtomicUsize::new(0);

        let shares = fetch_shares_for_ids(&announced, &claimed, |id| {
            fetch_calls.fetch_add(1, Ordering::Relaxed);
            Ok(share(id))
        });

        assert_eq!(sorted_ids(&shares.shares), vec!["a"]);
        assert_eq!(fetch_calls.load(Ordering::Relaxed), 1);
    }
}
