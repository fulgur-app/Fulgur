use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        share::{MAX_SYNC_SHARE_PAYLOAD_BYTES, fetch_pending_shares, fetch_share_by_id},
        synchronization::{
            SynchronizationError, SynchronizationStatus, set_sync_server_connection_status,
        },
    },
    utils::retry::BackoffCalculator,
};
use fulgur_common::api::shares::SharedFileResponse;
use parking_lot::Mutex;
use std::{
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use super::types::SseEvent;

/// Maximum size for SSE event data accumulation (10x payload limit to account for
/// base64 encoding overhead and JSON wrapper)
const MAX_SSE_EVENT_DATA_BYTES: usize = MAX_SYNC_SHARE_PAYLOAD_BYTES * 10;

/// Absolute deadline for receiving any byte on the SSE stream before the
/// connection is considered dead and an error is returned so the caller
/// reconnects
const SSE_READ_DEADLINE: Duration = Duration::from_secs(60);

/// HTTP header advertised by Fulgurant 0.7.0+ carrying the server version.
const FULGURANT_VERSION_HEADER: &str = "x-fulgurant-version";

/// Minimum Fulgurant `(major, minor)` version that supports fetching a single
/// share by id (`GET /api/shares/:id`) and advertises `x-fulgurant-version`.
const MIN_PER_ID_FETCH_VERSION: (u64, u64) = (0, 7);

/// Decide whether the server supports per-id share fetch from its advertised version.
///
/// ### Arguments
/// - `version_header`: The raw `x-fulgurant-version` value, if present.
///
/// ### Returns
/// - `true`: The server is recent enough to fetch shares by id.
/// - `false`: The header is absent, unparseable, or older than 0.7.0.
fn version_supports_per_id_fetch(version_header: Option<&str>) -> bool {
    let Some(raw) = version_header else {
        return false;
    };
    let trimmed = raw.trim().trim_start_matches('v');
    match semver::Version::parse(trimmed) {
        Ok(version) => (version.major, version.minor) >= MIN_PER_ID_FETCH_VERSION,
        Err(e) => {
            log::warn!("Unparseable {FULGURANT_VERSION_HEADER} header '{raw}': {e}");
            false
        }
    }
}

/// Decide whether a fetched share's encrypted payload is too large to queue,
/// based on the server-advertised max file size.
///
/// ### Arguments
/// - `content_len`: The encrypted payload length of the fetched share.
/// - `server_max_file_size`: The server-advertised max file size, or `u64::MAX`.
///
/// ### Returns
/// - `true`: The payload exceeds twice the server limit and should be dropped.
/// - `false`: The payload is within bounds, or the server advertises no limit.
fn share_payload_exceeds_limit(content_len: usize, server_max_file_size: u64) -> bool {
    server_max_file_size != u64::MAX && content_len as u64 > server_max_file_size.saturating_mul(2)
}

/// HTTP agents used by the SSE worker.
pub struct SseAgents {
    /// Short-timeout agent for REST calls (token, share fetches).
    pub rest: Arc<ureq::Agent>,
    /// Long-timeout agent for the long-lived SSE stream.
    pub stream: Arc<ureq::Agent>,
}

/// Per-profile shared state the SSE worker needs to drain pending shares.
pub struct SseShareState {
    /// Queue the UI tick drains incoming shares from.
    pub pending_shared_files: Arc<Mutex<Vec<SharedFileResponse>>>,
    /// Server-advertised max file size, used to bound the bulk drain response.
    pub max_file_size_bytes: Arc<AtomicU64>,
}

/// Error type for line reading with shutdown support
enum ReadError {
    /// I/O error during reading
    Io(std::io::Error),
    /// Shutdown was requested
    Shutdown,
}

/// Read a line from a buffered reader with periodic shutdown checks and an absolute read deadline.
///
/// ### Arguments
/// - `reader`: The buffered reader to read from
/// - `shutdown_flag`: Atomic flag to check for shutdown requests
///
/// ### Returns
/// - `Ok(Some(String))`: A line was read successfully
/// - `Ok(None)`: End of stream reached
/// - `Err(ReadError::Shutdown)`: Shutdown was requested
/// - `Err(ReadError::Io)`: I/O error occurred, the read deadline elapsed, or a
///   single line exceeded `MAX_SSE_EVENT_DATA_BYTES` (forcing a reconnect)
fn read_line_with_timeout<R: Read>(
    reader: &mut BufReader<R>,
    shutdown_flag: &Arc<AtomicBool>,
) -> Result<Option<String>, ReadError> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut last_byte_received = Instant::now();

    loop {
        if shutdown_flag.load(Ordering::Relaxed) {
            return Err(ReadError::Shutdown);
        }
        match reader.read(&mut byte) {
            Ok(0) => {
                if line.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
            }
            Ok(_) => {
                last_byte_received = Instant::now();
                if byte[0] == b'\n' {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
                }
                if line.len() >= MAX_SSE_EVENT_DATA_BYTES {
                    return Err(ReadError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "SSE line exceeds size limit ({MAX_SSE_EVENT_DATA_BYTES} bytes), connection presumed malicious"
                        ),
                    )));
                }
                line.push(byte[0]);
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                if last_byte_received.elapsed() > SSE_READ_DEADLINE {
                    return Err(ReadError::Io(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!(
                            "no data received within {}s, connection presumed dead",
                            SSE_READ_DEADLINE.as_secs()
                        ),
                    )));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return Err(ReadError::Io(e));
            }
        }
    }
}

/// Fetch pending shares from the server into the shared queue.
///
/// Will be deprecated in 0.11.0.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Arc to the per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_state`: Per-profile queue and server-advertised max file size
/// - `reason`: Short tag for logging ("reconnect" or "doorbell")
fn fetch_pending_shares_into(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    share_state: &SseShareState,
    reason: &str,
) {
    let server_max_file_size = share_state.max_file_size_bytes.load(Ordering::Acquire);
    match fetch_pending_shares(profile, token_state, http_agent, server_max_file_size) {
        Ok(shares) => {
            if shares.is_empty() {
                log::debug!("Fetch ({reason}): no pending shares");
                return;
            }
            let count = shares.len();
            let mut queue = share_state.pending_shared_files.lock();
            for share in shares {
                if share_payload_exceeds_limit(share.content.len(), server_max_file_size) {
                    log::warn!(
                        "Dropping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds 2x the server max ({} bytes)",
                        share.file_name,
                        share.source_device_id,
                        share.content.len(),
                        server_max_file_size
                    );
                    continue;
                }
                queue.push(share);
            }
            log::info!("Fetch ({reason}): queued {count} pending share(s)");
        }
        Err(e) => {
            log::warn!("Fetch ({reason}) failed: {e}");
        }
    }
}

/// Fetch a single share by id from a doorbell event into the shared queue.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Arc to the per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_state`: Per-profile queue and server-advertised max file size
/// - `share_id`: The id announced by the doorbell event
fn fetch_single_share_into(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    share_state: &SseShareState,
    share_id: &str,
) {
    let server_max_file_size = share_state.max_file_size_bytes.load(Ordering::Acquire);
    match fetch_share_by_id(
        profile,
        token_state,
        http_agent,
        share_id,
        server_max_file_size,
    ) {
        Ok(share) => {
            if share_payload_exceeds_limit(share.content.len(), server_max_file_size) {
                log::warn!(
                    "Dropping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds 2x the server max ({} bytes)",
                    share.file_name,
                    share.source_device_id,
                    share.content.len(),
                    server_max_file_size
                );
                return;
            }
            share_state.pending_shared_files.lock().push(share);
            log::info!("Fetch (doorbell): queued share id {share_id}");
        }
        Err(e) => {
            log::warn!("Fetch (doorbell) for id {share_id} failed: {e}");
        }
    }
}

/// Connect to SSE (Server-Sent Events) endpoint on the sync server for real-time notifications
///
/// ### Description
/// Establishes a persistent connection to the server's SSE endpoint to receive:
/// - Heartbeat events to keep connection alive
/// - Share notifications when files are shared from other devices
///
/// The connection runs in a background thread and automatically reconnects on failure.
///
/// ### Arguments
/// - `profile`: The server profile (URL, email, id) to connect to
/// - `event_tx`: Channel sender for sending SSE events to the main thread
/// - `shutdown_flag`: Atomic boolean flag to signal the SSE thread to shutdown
/// - `sync_server_connection_status`: Arc-wrapped connection status to update on connection/disconnection
/// - `token_state`: Arc to the per-profile token state manager for authentication
/// - `agents`: HTTP agents for the SSE stream and its REST calls
/// - `share_state`: Per-profile queue and server-advertised max file size
///
/// ### Errors
/// Returns a `SynchronizationError` if required profile fields (server URL,
/// email) are missing.
///
/// ### Returns
/// - `Ok(thread::JoinHandle<()>)`: If the SSE connection thread was spawned successfully
/// - `Err(SynchronizationError)`: If required profile fields are missing
pub fn connect_sse(
    profile: &ServerProfile,
    event_tx: Sender<SseEvent>,
    shutdown_flag: Arc<AtomicBool>,
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    token_state: &Arc<TokenStateManager>,
    agents: &SseAgents,
    share_state: &SseShareState,
) -> Result<thread::JoinHandle<()>, SynchronizationError> {
    let server_url = profile
        .server_url
        .clone()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    let sse_url = format!("{server_url}/api/sse");
    let profile_clone = profile.clone();
    let token_state_clone = Arc::clone(token_state);
    let http_agent_clone = Arc::clone(&agents.rest);
    let sse_http_agent_clone = Arc::clone(&agents.stream);
    let share_state_clone = SseShareState {
        pending_shared_files: Arc::clone(&share_state.pending_shared_files),
        max_file_size_bytes: Arc::clone(&share_state.max_file_size_bytes),
    };
    let handle = thread::spawn(move || {
        let mut backoff = BackoffCalculator::default_settings();

        loop {
            if shutdown_flag.load(Ordering::Relaxed) {
                log::info!("SSE connection shutdown requested, stopping...");
                break;
            }
            let token = match get_valid_token(&profile_clone, &token_state_clone, &http_agent_clone)
            {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Failed to get valid token for SSE: {e}");
                    set_sync_server_connection_status(
                        &sync_server_connection_status,
                        SynchronizationStatus::AuthenticationFailed,
                    );
                    let delay = backoff.record_failure();
                    log::info!("Retrying SSE connection after {delay:?}");
                    thread::sleep(delay);
                    continue;
                }
            };
            log::info!("Connecting to SSE endpoint: {sse_url}");
            let response = match sse_http_agent_clone
                .get(&sse_url)
                .header("Authorization", &format!("Bearer {token}"))
                .header("Accept", "text/event-stream")
                .call()
            {
                Ok(resp) => {
                    set_sync_server_connection_status(
                        &sync_server_connection_status,
                        SynchronizationStatus::Connected,
                    );
                    log::info!("SSE connection established");
                    backoff.record_success();
                    fetch_pending_shares_into(
                        &profile_clone,
                        &token_state_clone,
                        &http_agent_clone,
                        &share_state_clone,
                        "reconnect",
                    );
                    resp
                }
                Err(e) => {
                    log::error!("SSE connection failed: {e}");
                    set_sync_server_connection_status(
                        &sync_server_connection_status,
                        SynchronizationStatus::Disconnected,
                    );
                    event_tx.send(SseEvent::Error(e.to_string())).ok();
                    if shutdown_flag.load(Ordering::Relaxed) {
                        log::info!("SSE connection shutdown requested, stopping...");
                        break;
                    }
                    let delay = backoff.record_failure();
                    log::info!("Retrying SSE connection after {delay:?}");
                    thread::sleep(delay);
                    continue;
                }
            };
            let mut response = response;
            let supports_per_id_fetch = version_supports_per_id_fetch(
                response
                    .headers()
                    .get(FULGURANT_VERSION_HEADER)
                    .and_then(|value| value.to_str().ok()),
            );
            if supports_per_id_fetch {
                log::info!("Server supports per-id share fetch; doorbell events fetch by id");
            } else {
                log::info!("Server lacks per-id share fetch; doorbell events use bulk drain");
            }
            let mut reader = std::io::BufReader::new(response.body_mut().as_reader());
            let mut current_event_type = String::new();
            let mut current_data = String::new();
            let mut receiver_gone = false;

            loop {
                if shutdown_flag.load(Ordering::Relaxed) {
                    log::info!(
                        "SSE connection shutdown requested during event reading, stopping..."
                    );
                    break;
                }

                let line_result = read_line_with_timeout(&mut reader, &shutdown_flag);
                match line_result {
                    Ok(Some(line)) => {
                        if line.starts_with("event:") {
                            current_event_type =
                                line.trim_start_matches("event:").trim().to_string();
                        } else if line.starts_with("data:") {
                            let fragment = line.trim_start_matches("data:").trim();
                            if current_data.len() + fragment.len() > MAX_SSE_EVENT_DATA_BYTES {
                                log::warn!(
                                    "SSE event data exceeds size limit ({MAX_SSE_EVENT_DATA_BYTES} bytes), discarding"
                                );
                                current_data.clear();
                                current_event_type.clear();
                                continue;
                            }
                            current_data.push_str(fragment);
                        } else if line.is_empty() && !current_data.is_empty() {
                            log::info!("SSE event type: {current_event_type}");
                            log::debug!("SSE event received ({} bytes)", current_data.len());
                            let event = SseEvent::parse(&current_event_type, &current_data);
                            if let SseEvent::ShareAvailable(ref notification) = event {
                                log::info!(
                                    "Share doorbell received (share_id={}), fetching share",
                                    notification.share_id
                                );
                                if supports_per_id_fetch {
                                    fetch_single_share_into(
                                        &profile_clone,
                                        &token_state_clone,
                                        &http_agent_clone,
                                        &share_state_clone,
                                        &notification.share_id,
                                    );
                                } else {
                                    //TODO: Remove in 0.11.0
                                    fetch_pending_shares_into(
                                        &profile_clone,
                                        &token_state_clone,
                                        &http_agent_clone,
                                        &share_state_clone,
                                        "doorbell",
                                    );
                                }
                            }
                            if let Err(e) = event_tx.send(event) {
                                log::error!("Failed to send SSE event: {e}");
                                receiver_gone = true;
                                break;
                            }
                            current_event_type.clear();
                            current_data.clear();
                        }
                    }
                    Ok(None) => {
                        log::info!("SSE stream ended");
                        break;
                    }
                    Err(ReadError::Shutdown) => {
                        log::info!("SSE connection shutdown requested");
                        break;
                    }
                    Err(ReadError::Io(e)) => {
                        log::error!("SSE stream error: {e}");
                        set_sync_server_connection_status(
                            &sync_server_connection_status,
                            SynchronizationStatus::Disconnected,
                        );
                        event_tx.send(SseEvent::Error(e.to_string())).ok();
                        break;
                    }
                }
            }
            if shutdown_flag.load(Ordering::Relaxed) {
                log::info!("SSE connection shutdown requested, stopping...");
                break;
            }
            if receiver_gone {
                log::info!("SSE event receiver permanently gone, stopping worker");
                break;
            }
            let delay = backoff.record_failure();
            log::warn!("SSE connection closed, reconnecting after {delay:?}");
            set_sync_server_connection_status(
                &sync_server_connection_status,
                SynchronizationStatus::Disconnected,
            );
            thread::sleep(delay);
        }
    });
    Ok(handle)
}

#[cfg(test)]
mod tests {
    use super::{share_payload_exceeds_limit, version_supports_per_id_fetch};

    #[test]
    fn unlimited_server_never_drops() {
        assert!(!share_payload_exceeds_limit(usize::MAX, u64::MAX));
    }

    #[test]
    fn payload_within_twice_the_limit_is_kept() {
        let server_max = 1024;
        assert!(!share_payload_exceeds_limit(2 * 1024, server_max));
    }

    #[test]
    fn payload_above_twice_the_limit_is_dropped() {
        let server_max = 1024;
        assert!(share_payload_exceeds_limit(2 * 1024 + 1, server_max));
    }

    #[test]
    fn absent_header_falls_back_to_bulk() {
        assert!(!version_supports_per_id_fetch(None));
    }

    #[test]
    fn unparseable_header_falls_back_to_bulk() {
        assert!(!version_supports_per_id_fetch(Some("not-a-version")));
    }

    #[test]
    fn exact_minimum_version_is_supported() {
        assert!(version_supports_per_id_fetch(Some("0.7.0")));
    }

    #[test]
    fn older_version_is_not_supported() {
        assert!(!version_supports_per_id_fetch(Some("0.6.9")));
    }

    #[test]
    fn newer_minor_and_major_are_supported() {
        assert!(version_supports_per_id_fetch(Some("0.8.1")));
        assert!(version_supports_per_id_fetch(Some("1.0.0")));
    }

    #[test]
    fn leading_v_and_whitespace_are_tolerated() {
        assert!(version_supports_per_id_fetch(Some("  v0.7.0  ")));
    }
}
