use crate::fulgur::{
    settings::SynchronizationSettings,
    sync::{
        access_token::{TokenStateManager, get_valid_token},
        share::{MAX_SYNC_SHARE_PAYLOAD_BYTES, fetch_pending_shares},
        synchronization::{
            SynchronizationError, SynchronizationStatus, set_sync_server_connection_status,
        },
    },
    utils::{retry::BackoffCalculator, sanitize::sanitize_filename},
};
use fulgur_common::api::shares::SharedFileResponse;
use parking_lot::Mutex;
use std::{
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::Duration,
};

use super::types::SseEvent;

/// Maximum size for SSE event data accumulation (10x payload limit to account for
/// base64 encoding overhead and JSON wrapper)
const MAX_SSE_EVENT_DATA_BYTES: usize = MAX_SYNC_SHARE_PAYLOAD_BYTES * 10;

/// Error type for line reading with shutdown support
enum ReadError {
    /// I/O error during reading
    Io(std::io::Error),
    /// Shutdown was requested
    Shutdown,
}

/// Read a line from a buffered reader with periodic shutdown checks
///
/// ### Arguments
/// - `reader`: The buffered reader to read from
/// - `shutdown_flag`: Atomic flag to check for shutdown requests
///
/// ### Returns
/// - `Ok(Some(String))`: A line was read successfully
/// - `Ok(None)`: End of stream reached
/// - `Err(ReadError::Shutdown)`: Shutdown was requested
/// - `Err(ReadError::Io)`: I/O error occurred
fn read_line_with_timeout<R: Read>(
    reader: &mut BufReader<R>,
    shutdown_flag: &Arc<AtomicBool>,
) -> Result<Option<String>, ReadError> {
    let mut line = String::new();
    let mut byte = [0u8; 1];

    loop {
        if shutdown_flag.load(Ordering::Relaxed) {
            return Err(ReadError::Shutdown);
        }
        match reader.read(&mut byte) {
            Ok(0) => {
                if line.is_empty() {
                    return Ok(None);
                } else {
                    return Ok(Some(line));
                }
            }
            Ok(_) => {
                if byte[0] == b'\n' {
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    return Ok(Some(line));
                } else {
                    line.push(byte[0] as char);
                }
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut =>
            {
                thread::sleep(Duration::from_millis(10));
                continue;
            }
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {
                continue;
            }
            Err(e) => {
                return Err(ReadError::Io(e));
            }
        }
    }
}

/// Fetch pending shares from the server into the shared queue.
///
/// ### Arguments
/// - `synchronization_settings`: The synchronization settings
/// - `token_state`: Arc to the token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `pending_shared_files`: Shared queue that the UI tick drains
/// - `reason`: Short tag for logging ("reconnect" or "doorbell")
fn fetch_pending_shares_into(
    synchronization_settings: &SynchronizationSettings,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    pending_shared_files: &Arc<Mutex<Vec<SharedFileResponse>>>,
    reason: &str,
) {
    match fetch_pending_shares(synchronization_settings, token_state, http_agent) {
        Ok(shares) => {
            if shares.is_empty() {
                log::debug!("Fetch ({reason}): no pending shares");
                return;
            }
            let count = shares.len();
            let mut queue = pending_shared_files.lock();
            for mut share in shares {
                if share.content.len() > MAX_SYNC_SHARE_PAYLOAD_BYTES {
                    log::warn!(
                        "Dropping shared file '{}' from device {}: encrypted payload exceeds {} bytes",
                        share.file_name,
                        share.source_device_id,
                        MAX_SYNC_SHARE_PAYLOAD_BYTES
                    );
                    continue;
                }
                share.file_name = sanitize_filename(&share.file_name);
                queue.push(share);
            }
            log::info!("Fetch ({reason}): queued {count} pending share(s)");
        }
        Err(e) => {
            log::warn!("Fetch ({reason}) failed: {e}");
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
/// - `synchronization_settings`: The synchronization settings containing server URL, email, and key
/// - `event_tx`: Channel sender for sending SSE events to the main thread
/// - `shutdown_flag`: Atomic boolean flag to signal the SSE thread to shutdown
/// - `sync_server_connection_status`: Arc-wrapped connection status to update on connection/disconnection
/// - `token_state`: Arc to the token state manager for authentication
/// - `http_agent`: Shared HTTP agent for connection pooling
/// - `pending_shared_files`: Shared queue for incoming file shares
///
/// ### Returns
/// - `Ok(thread::JoinHandle<()>)`: If the SSE connection thread was spawned successfully
/// - `Err(SynchronizationError)`: If required settings are missing
pub fn connect_sse(
    synchronization_settings: &SynchronizationSettings,
    event_tx: Sender<SseEvent>,
    shutdown_flag: Arc<AtomicBool>,
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    token_state: &Arc<TokenStateManager>,
    http_agent: &Arc<ureq::Agent>,
    pending_shared_files: &Arc<Mutex<Vec<SharedFileResponse>>>,
) -> Result<thread::JoinHandle<()>, SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .clone()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    let sse_url = format!("{server_url}/api/sse");
    let settings_clone = synchronization_settings.clone();
    let token_state_clone = Arc::clone(token_state);
    let http_agent_clone = Arc::clone(http_agent);
    let pending_shared_files_clone = Arc::clone(pending_shared_files);
    let handle = thread::spawn(move || {
        let mut backoff = BackoffCalculator::default_settings();

        loop {
            if shutdown_flag.load(Ordering::Relaxed) {
                log::info!("SSE connection shutdown requested, stopping...");
                break;
            }
            let token =
                match get_valid_token(&settings_clone, &token_state_clone, &http_agent_clone) {
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
            let response = match http_agent_clone
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
                        &settings_clone,
                        &token_state_clone,
                        &http_agent_clone,
                        &pending_shared_files_clone,
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
            let mut reader = std::io::BufReader::new(response.body_mut().as_reader());
            let mut current_event_type = String::new();
            let mut current_data = String::new();

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
                                    "Share doorbell received (share_id={}), fetching pending shares",
                                    notification.share_id
                                );
                                fetch_pending_shares_into(
                                    &settings_clone,
                                    &token_state_clone,
                                    &http_agent_clone,
                                    &pending_shared_files_clone,
                                    "doorbell",
                                );
                            }
                            if let Err(e) = event_tx.send(event) {
                                log::error!("Failed to send SSE event: {e}");
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
