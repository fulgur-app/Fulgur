use crate::fulgur::{
    settings::ServerProfile,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        share::{MAX_SYNC_SHARE_PAYLOAD_BYTES, fetch_share_by_id},
        synchronization::{
            FULGURANT_VERSION_HEADER, MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY,
            SynchronizationError, SynchronizationStatus, list_pending_share_ids,
            server_meets_minimum_version, set_sync_server_connection_status,
        },
    },
    utils::{
        retry::{BackoffCalculator, interruptible_sleep},
        worker::WorkerHooks,
    },
};
use fulgur_common::api::shares::SharedFileResponse;
use futures::channel::mpsc::UnboundedSender;
use parking_lot::Mutex;
use std::{
    collections::HashSet,
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, AtomicU64, Ordering},
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
const SSE_READ_DEADLINE: Duration = Duration::from_mins(1);

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
    /// Share IDs fetched via the v2 read/ack flow awaiting acknowledgement once
    /// decryption succeeds. The bulk drain skips IDs present here.
    pub pending_ack_share_ids: Arc<Mutex<HashSet<String>>>,
    /// Server-advertised max file size, used to bound the bulk drain response.
    pub max_file_size_bytes: Arc<AtomicU64>,
    /// Raw `x-fulgurant-version` value, updated each time the SSE handshake
    /// succeeds. `None` means the server did not advertise a version.
    pub server_version: Arc<Mutex<Option<String>>>,
}

/// Publish a connection status and wake the windows displaying it.
///
/// ### Arguments
/// - `shutdown_flag`: The worker's shutdown flag.
/// - `connection_status`: The profile's shared connection status.
/// - `event_tx`: Channel carrying the change to the UI-thread consumer.
/// - `status`: The status to publish.
fn publish_connection_status(
    shutdown_flag: &AtomicBool,
    connection_status: &Arc<Mutex<SynchronizationStatus>>,
    event_tx: &UnboundedSender<SseEvent>,
    status: SynchronizationStatus,
) {
    if shutdown_flag.load(Ordering::Relaxed) {
        return;
    }
    if *connection_status.lock() == status {
        return;
    }
    set_sync_server_connection_status(connection_status, status);
    event_tx
        .unbounded_send(SseEvent::ConnectionStatusChanged)
        .ok();
}

/// Consecutive failed attempts still reported as "connecting" before a profile
/// is shown as disconnected.
const TRANSIENT_RETRY_ATTEMPTS: u32 = 5;

/// Resolve the status to publish after a failed attempt that will be retried.
///
/// ### Arguments
/// - `consecutive_failures`: Failed attempts since the last successful connect.
///
/// ### Returns
/// - `None`: The outage still looks transient; leave the status as it is.
/// - `Some(SynchronizationStatus::Disconnected)`: Retries have gone on long
///   enough that the profile should be reported as down.
fn retry_status(consecutive_failures: u32) -> Option<SynchronizationStatus> {
    (consecutive_failures > TRANSIENT_RETRY_ATTEMPTS).then_some(SynchronizationStatus::Disconnected)
}

/// Status Fulgurant answers with while this device's previous SSE stream still
/// holds a slot against the per-device connection cap.
const SSE_SLOT_BUSY_STATUS: u16 = 429;

/// Delay between attempts while the previous stream is still releasing its slot.
const SSE_SLOT_BUSY_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Attempts spent at the fast handoff cadence before the standard backoff takes over.
const SSE_SLOT_BUSY_FAST_ATTEMPTS: u32 = 10;

/// Decide whether a failed connect is a slot handoff worth retrying quickly.
///
/// ### Arguments
/// - `error`: The error the connect attempt failed with.
/// - `attempts_so_far`: Fast handoff attempts already spent since the last success.
///
/// ### Returns
/// - `true`: The previous stream is still releasing its slot; retry quickly.
/// - `false`: Treat the attempt as a real failure and back off.
fn is_handoff_retry(error: &ureq::Error, attempts_so_far: u32) -> bool {
    matches!(error, ureq::Error::StatusCode(SSE_SLOT_BUSY_STATUS))
        && attempts_so_far < SSE_SLOT_BUSY_FAST_ATTEMPTS
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

/// Fetch a single share by id via the read/ack flow into the shared queue.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Arc to the per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_state`: Per-profile queue, ack set, and server-advertised max file size
/// - `share_id`: The id announced by the doorbell event
fn fetch_single_share_into(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    share_state: &SseShareState,
    share_id: &str,
) {
    if share_state.pending_ack_share_ids.lock().contains(share_id) {
        log::debug!("Fetch (doorbell): share id {share_id} already in flight, skipping");
        return;
    }
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
            share_state
                .pending_ack_share_ids
                .lock()
                .insert(share.id.clone());
            share_state.pending_shared_files.lock().push(share);
            log::info!("Fetch (doorbell): queued share id {share_id}, pending ack");
        }
        Err(e) => {
            log::warn!("Fetch (doorbell) for id {share_id} failed: {e}");
        }
    }
}

/// Catch up on pending shares after an SSE reconnect.
///
/// ### Description
/// The server does not replay doorbell events for shares that arrived while the
/// connection was down, so the catch-up enumerates the device's pending share
/// ids with the non-consuming `POST /api/v2/begin` (Fulgurant exposes no
/// standalone listing endpoint) and routes each through
/// `fetch_single_share_into`, which deduplicates against in-flight doorbell
/// fetches and registers ids for acknowledgement after a successful download.
///
/// ### Arguments
/// - `profile`: The server profile to fetch from
/// - `token_state`: Arc to the per-profile token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `share_state`: Per-profile queue, ack set, and server-advertised max file size
fn fetch_pending_shares_into(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    share_state: &SseShareState,
) {
    match list_pending_share_ids(profile, token_state, http_agent) {
        Ok(share_ids) => {
            if share_ids.is_empty() {
                log::debug!("Fetch (reconnect): no pending shares");
                return;
            }
            let count = share_ids.len();
            for share_id in &share_ids {
                fetch_single_share_into(profile, token_state, http_agent, share_state, share_id);
            }
            log::info!("Fetch (reconnect): processed {count} pending share id(s)");
        }
        Err(e) => {
            log::warn!("Fetch (reconnect) failed: {e}");
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
/// - `hooks`: Shutdown flag polled by the thread and slot receiving its `JoinHandle`
/// - `sync_server_connection_status`: Arc-wrapped connection status to update on connection/disconnection
/// - `token_state`: Arc to the per-profile token state manager for authentication
/// - `agents`: HTTP agents for the SSE stream and its REST calls
/// - `share_state`: Per-profile queue and server-advertised max file size
///
/// ### Errors
/// Returns a `SynchronizationError` if required profile fields (server URL,
/// email) are missing or the OS refuses to spawn the thread.
///
/// ### Returns
/// - `Ok(())`: The SSE connection thread was spawned and attached to the hooks
/// - `Err(SynchronizationError)`: Required profile fields are missing or the spawn failed
pub fn connect_sse(
    profile: &ServerProfile,
    event_tx: UnboundedSender<SseEvent>,
    hooks: &WorkerHooks,
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    token_state: &Arc<TokenStateManager>,
    agents: &SseAgents,
    share_state: &SseShareState,
) -> Result<(), SynchronizationError> {
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
        pending_ack_share_ids: Arc::clone(&share_state.pending_ack_share_ids),
        max_file_size_bytes: Arc::clone(&share_state.max_file_size_bytes),
        server_version: Arc::clone(&share_state.server_version),
    };
    let shutdown_flag = Arc::clone(&hooks.shutdown_flag);
    let handle = thread::Builder::new()
        .name(format!("fulgur-sse-{}", profile.name))
        .spawn(move || {
        let mut backoff = BackoffCalculator::default_settings();
        let mut consecutive_failures: u32 = 0;
        let mut handoff_attempts: u32 = 0;

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
                    publish_connection_status(
                        &shutdown_flag,
                        &sync_server_connection_status,
                        &event_tx,
                        SynchronizationStatus::AuthenticationFailed,
                    );
                    let delay = backoff.record_failure();
                    log::info!("Retrying SSE connection after {delay:?}");
                    if interruptible_sleep(delay, || shutdown_flag.load(Ordering::Relaxed)) {
                        log::info!("SSE connection shutdown requested during backoff, stopping...");
                        break;
                    }
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
                Ok(resp) => resp,
                Err(e) => {
                    // A slot handoff is not a failed connection: the profile is
                    // simply waiting for the stream it replaces to be torn down,
                    // so it must neither be reported as an error nor counted
                    // towards the disconnected threshold.
                    let delay = if is_handoff_retry(&e, handoff_attempts) {
                        handoff_attempts = handoff_attempts.saturating_add(1);
                        log::info!(
                            "SSE slot still held by the previous stream, retrying in {SSE_SLOT_BUSY_RETRY_DELAY:?}"
                        );
                        SSE_SLOT_BUSY_RETRY_DELAY
                    } else {
                        log::error!("SSE connection failed: {e}");
                        consecutive_failures = consecutive_failures.saturating_add(1);
                        if let Some(status) = retry_status(consecutive_failures) {
                            publish_connection_status(
                                &shutdown_flag,
                                &sync_server_connection_status,
                                &event_tx,
                                status,
                            );
                        }
                        event_tx.unbounded_send(SseEvent::Error(e.to_string())).ok();
                        backoff.record_failure()
                    };
                    if shutdown_flag.load(Ordering::Relaxed) {
                        log::info!("SSE connection shutdown requested, stopping...");
                        break;
                    }
                    log::info!("Retrying SSE connection after {delay:?}");
                    if interruptible_sleep(delay, || shutdown_flag.load(Ordering::Relaxed)) {
                        log::info!("SSE connection shutdown requested during backoff, stopping...");
                        break;
                    }
                    continue;
                }
            };
            let mut response = response;
            let version_header = response
                .headers()
                .get(FULGURANT_VERSION_HEADER)
                .and_then(|value| value.to_str().ok())
                .map(str::to_owned);
            (*share_state_clone.server_version.lock()).clone_from(&version_header);
            if !server_meets_minimum_version(version_header.as_deref()) {
                let error = SynchronizationError::ServerTooOld {
                    required: MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY,
                };
                log::error!(
                    "Server advertises Fulgurant version {}: {error}",
                    version_header.as_deref().unwrap_or("<none>")
                );
                publish_connection_status(
                    &shutdown_flag,
                    &sync_server_connection_status,
                    &event_tx,
                    SynchronizationStatus::from_error(&error),
                );
                event_tx
                    .unbounded_send(SseEvent::Error(error.to_string()))
                    .ok();
                let delay = backoff.record_failure();
                log::info!("Retrying SSE connection after {delay:?}");
                if interruptible_sleep(delay, || shutdown_flag.load(Ordering::Relaxed)) {
                    log::info!("SSE connection shutdown requested during backoff, stopping...");
                    break;
                }
                continue;
            }
            publish_connection_status(
                &shutdown_flag,
                &sync_server_connection_status,
                &event_tx,
                SynchronizationStatus::Connected,
            );
            log::info!("SSE connection established");
            backoff.record_success();
            consecutive_failures = 0;
            handoff_attempts = 0;
            // Catch up on shares that arrived while the connection was down. The
            // server does not replay doorbell events for the downtime window.
            fetch_pending_shares_into(
                &profile_clone,
                &token_state_clone,
                &http_agent_clone,
                &share_state_clone,
            );
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
                                fetch_single_share_into(
                                    &profile_clone,
                                    &token_state_clone,
                                    &http_agent_clone,
                                    &share_state_clone,
                                    &notification.share_id,
                                );
                            }
                            if let Err(e) = event_tx.unbounded_send(event) {
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
                        // The shared "connection closed, reconnecting" path
                        // below publishes the status for every stream drop.
                        log::error!("SSE stream error: {e}");
                        event_tx.unbounded_send(SseEvent::Error(e.to_string())).ok();
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
            consecutive_failures = consecutive_failures.saturating_add(1);
            log::warn!("SSE connection closed, reconnecting after {delay:?}");
            if let Some(status) = retry_status(consecutive_failures) {
                publish_connection_status(
                    &shutdown_flag,
                    &sync_server_connection_status,
                    &event_tx,
                    status,
                );
            }
            if interruptible_sleep(delay, || shutdown_flag.load(Ordering::Relaxed)) {
                log::info!("SSE connection shutdown requested during backoff, stopping...");
                break;
            }
        }
    })
        .map_err(|e| SynchronizationError::Other(format!("failed to spawn SSE thread: {e}")))?;
    *hooks.handle_slot.lock() = Some(handle);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        Arc, AtomicBool, Mutex, Ordering, SSE_SLOT_BUSY_FAST_ATTEMPTS, SSE_SLOT_BUSY_STATUS,
        SseEvent, SynchronizationStatus, TRANSIENT_RETRY_ATTEMPTS, is_handoff_retry,
        publish_connection_status, retry_status, share_payload_exceeds_limit,
    };
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};

    /// Count the `ConnectionStatusChanged` events queued on a receiver.
    fn queued_status_changes(events: &mut UnboundedReceiver<SseEvent>) -> usize {
        let mut count = 0;
        while let Ok(event) = events.try_recv() {
            if matches!(event, SseEvent::ConnectionStatusChanged) {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn a_running_worker_publishes_its_status_and_wakes_the_ui() {
        let shutdown = AtomicBool::new(false);
        let status = Arc::new(Mutex::new(SynchronizationStatus::Connecting));
        let (tx, mut rx) = unbounded();
        publish_connection_status(&shutdown, &status, &tx, SynchronizationStatus::Connected);
        assert_eq!(*status.lock(), SynchronizationStatus::Connected);
        assert_eq!(
            queued_status_changes(&mut rx),
            1,
            "a status change must be announced so the windows repaint"
        );
    }

    #[test]
    fn republishing_the_same_status_does_not_wake_the_ui() {
        let shutdown = AtomicBool::new(false);
        let status = Arc::new(Mutex::new(SynchronizationStatus::Disconnected));
        let (tx, mut rx) = unbounded();
        publish_connection_status(&shutdown, &status, &tx, SynchronizationStatus::Disconnected);
        assert_eq!(
            queued_status_changes(&mut rx),
            0,
            "a reconnect loop must not wake the UI on every retry"
        );
    }

    #[test]
    fn a_retiring_worker_cannot_overwrite_its_replacement() {
        // The replacement worker has published Connected on the shared cell;
        // the worker being retired must not downgrade it on its way out.
        let shutdown = AtomicBool::new(false);
        let status = Arc::new(Mutex::new(SynchronizationStatus::Connected));
        let (tx, mut rx) = unbounded();
        shutdown.store(true, Ordering::Relaxed);
        publish_connection_status(&shutdown, &status, &tx, SynchronizationStatus::Disconnected);
        assert_eq!(
            *status.lock(),
            SynchronizationStatus::Connected,
            "a worker asked to stop must not write to the shared status cell"
        );
        assert_eq!(queued_status_changes(&mut rx), 0);
    }

    #[test]
    fn a_reconnect_in_progress_leaves_the_status_alone() {
        // The profile is usable once the initial synchronization succeeds, so a
        // stream reconnecting in the background must not pull it out of
        // Connected and contradict the notification the user just saw.
        for attempt in 1..=TRANSIENT_RETRY_ATTEMPTS {
            assert_eq!(
                retry_status(attempt),
                None,
                "attempt {attempt} is still within the transient window"
            );
        }
    }

    #[test]
    fn a_sustained_outage_reads_as_disconnected() {
        assert_eq!(
            retry_status(TRANSIENT_RETRY_ATTEMPTS + 1),
            Some(SynchronizationStatus::Disconnected),
            "retries that stop looking transient must report the profile as down"
        );
    }

    #[test]
    fn a_busy_slot_is_retried_on_the_fast_cadence() {
        // The server answers 429 while the stream being replaced still counts
        // against the per-device cap. Backing off exponentially there would
        // overshoot a release that happens within a moment.
        let busy = ureq::Error::StatusCode(SSE_SLOT_BUSY_STATUS);
        assert!(is_handoff_retry(&busy, 0));
        assert!(is_handoff_retry(&busy, SSE_SLOT_BUSY_FAST_ATTEMPTS - 1));
    }

    #[test]
    fn a_slot_that_never_frees_falls_back_to_backoff() {
        let busy = ureq::Error::StatusCode(SSE_SLOT_BUSY_STATUS);
        assert!(
            !is_handoff_retry(&busy, SSE_SLOT_BUSY_FAST_ATTEMPTS),
            "the fast cadence must be bounded so a stuck slot stops being polled"
        );
    }

    #[test]
    fn a_real_failure_is_never_treated_as_a_handoff() {
        assert!(!is_handoff_retry(&ureq::Error::StatusCode(500), 0));
        assert!(!is_handoff_retry(&ureq::Error::HostNotFound, 0));
    }

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
}
