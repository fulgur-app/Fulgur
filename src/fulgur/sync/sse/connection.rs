mod fetch;
mod reader;
mod status;

use crate::fulgur::{
    settings::ServerProfile,
    shared_state::AppNotification,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        synchronization::{
            FULGURANT_VERSION_HEADER, MIN_SUPPORTED_FULGURANT_VERSION_DISPLAY,
            SynchronizationError, SynchronizationStatus, server_meets_minimum_version,
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
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
    thread,
};

use super::types::SseEvent;
use fetch::{fetch_pending_shares_into, fetch_single_share_into};
use reader::{MAX_SSE_EVENT_DATA_BYTES, ReadError, read_line_with_timeout};
use status::{
    SSE_SLOT_BUSY_RETRY_DELAY, is_handoff_retry, publish_connection_status, retry_status,
};

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
    /// Claimed share ids, never fetched again until acknowledged (see
    /// `SyncState::pending_ack_share_ids`).
    pub pending_ack_share_ids: Arc<Mutex<HashSet<String>>>,
    /// Server-advertised max file size, used to bound the bulk drain response.
    pub max_file_size_bytes: Arc<AtomicU64>,
    /// Raw `x-fulgurant-version` value, updated each time the SSE handshake
    /// succeeds. `None` means the server did not advertise a version.
    pub server_version: Arc<Mutex<Option<String>>>,
    /// Sender for user-facing notifications, used to report shares rejected
    /// for a permanent download failure (see `SyncState::notification_tx`).
    pub notification_tx: UnboundedSender<AppNotification>,
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
        notification_tx: share_state.notification_tx.clone(),
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
