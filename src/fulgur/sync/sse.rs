use gpui::App;
use std::{
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use gpui::{Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use parking_lot::Mutex;
use serde::Serialize;

use crate::fulgur::{
    Fulgur,
    settings::SynchronizationSettings,
    sync::{
        access_token::{TokenState, get_valid_token},
        synchronization::{
            SynchronizationError, SynchronizationStatus, set_sync_server_connection_status,
        },
    },
    utils::retry::BackoffCalculator,
    utils::sanitize::sanitize_filename,
};

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
                // End of stream
                if line.is_empty() {
                    return Ok(None);
                } else {
                    return Ok(Some(line));
                }
            }
            Ok(_) => {
                if byte[0] == b'\n' {
                    // End of line (handle both \n and \r\n)
                    if line.ends_with('\r') {
                        line.pop();
                    }
                    return Ok(Some(line));
                } else {
                    line.push(byte[0] as char);
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                // Non-blocking read would block, sleep briefly and retry
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

/// Connect to SSE (Server-Sent Events) endpoint on the sync serverfor real-time notifications
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
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(())`: If the SSE connection thread was spawned successfully
/// - `Err(SynchronizationError)`: If required settings are missing
pub fn connect_sse(
    synchronization_settings: &SynchronizationSettings,
    event_tx: Sender<SseEvent>,
    shutdown_flag: Arc<AtomicBool>,
    sync_server_connection_status: Arc<Mutex<SynchronizationStatus>>,
    token_state: Arc<Mutex<TokenState>>,
    http_agent: Arc<ureq::Agent>,
) -> Result<(), SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .clone()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    let sse_url = format!("{}/api/sse", server_url);
    let settings_clone = synchronization_settings.clone();
    let token_state_clone = Arc::clone(&token_state);
    let http_agent_clone = Arc::clone(&http_agent);
    thread::spawn(move || {
        // Exponential backoff for reconnection attempts (1s, 2s, 4s, 8s... up to 5 minutes)
        let mut backoff = BackoffCalculator::default_settings();

        loop {
            if shutdown_flag.load(Ordering::Relaxed) {
                log::info!("SSE connection shutdown requested, stopping...");
                break;
            }
            let token = match get_valid_token(
                &settings_clone,
                Arc::clone(&token_state_clone),
                &http_agent_clone,
            ) {
                Ok(t) => t,
                Err(e) => {
                    log::error!("Failed to get valid token for SSE: {}", e);
                    set_sync_server_connection_status(
                        sync_server_connection_status.clone(),
                        SynchronizationStatus::AuthenticationFailed,
                    );
                    let delay = backoff.record_failure();
                    log::info!("Retrying SSE connection after {:?}", delay);
                    thread::sleep(delay);
                    continue;
                }
            };
            log::info!("Connecting to SSE endpoint: {}", sse_url);
            let response = match http_agent_clone
                .get(&sse_url)
                .header("Authorization", &format!("Bearer {}", token))
                .header("Accept", "text/event-stream")
                .call()
            {
                Ok(resp) => {
                    set_sync_server_connection_status(
                        sync_server_connection_status.clone(),
                        SynchronizationStatus::Connected,
                    );
                    log::info!("SSE connection established");
                    backoff.record_success(); // Reset backoff on successful connection
                    resp
                }
                Err(e) => {
                    log::error!("SSE connection failed: {}", e);
                    set_sync_server_connection_status(
                        sync_server_connection_status.clone(),
                        SynchronizationStatus::Disconnected,
                    );
                    event_tx.send(SseEvent::Error(e.to_string())).ok();
                    if shutdown_flag.load(Ordering::Relaxed) {
                        log::info!("SSE connection shutdown requested, stopping...");
                        break;
                    }
                    let delay = backoff.record_failure();
                    log::info!("Retrying SSE connection after {:?}", delay);
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
                            current_data.push_str(line.trim_start_matches("data:").trim());
                        } else if line.is_empty() && !current_data.is_empty() {
                            log::info!("SSE event type: {}", current_event_type);
                            log::info!("SSE data: {}", current_data);
                            let event = SseEvent::parse(&current_event_type, &current_data);
                            if let Err(e) = event_tx.send(event) {
                                log::error!("Failed to send SSE event: {}", e);
                                break;
                            }
                            current_event_type.clear();
                            current_data.clear();
                        }
                    }
                    Ok(None) => {
                        // End of stream
                        log::info!("SSE stream ended");
                        break;
                    }
                    Err(ReadError::Shutdown) => {
                        log::info!("SSE connection shutdown requested");
                        break;
                    }
                    Err(ReadError::Io(e)) => {
                        log::error!("SSE stream error: {}", e);
                        set_sync_server_connection_status(
                            sync_server_connection_status.clone(),
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
            // When the connection is lost
            let delay = backoff.record_failure();
            log::warn!("SSE connection closed, reconnecting after {:?}", delay);
            set_sync_server_connection_status(
                sync_server_connection_status.clone(),
                SynchronizationStatus::Disconnected,
            );
            thread::sleep(delay);
        }
    });

    Ok(())
}

/// Heartbeat event data sent by SSE to keep connection alive
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct HeartbeatData {
    pub timestamp: String,
}

/// Share notification event data sent by SSE when a file is shared
#[derive(Debug, Clone, Serialize, serde::Deserialize)]
pub struct ShareNotification {
    pub share_id: String,
    pub source_device_id: String,
    pub destination_device_id: String,
    pub file_name: String,
    pub file_size: i64,
    pub file_hash: String,
    pub content: String, // Encrypted and base64 encoded
    pub created_at: String,
    pub expires_at: String,
}

/// SSE Event types that can be received from the server
#[derive(Debug, Clone)]
pub enum SseEvent {
    /// Heartbeat event to keep connection alive (sent every ~30s)
    Heartbeat { timestamp: String },
    /// File share notification with full share details
    ShareAvailable(ShareNotification),
    /// Error event for connection or parsing errors
    Error(String),
}

impl SseEvent {
    /// Parse an SSE event from the event type and data
    ///
    /// ### Arguments
    /// - `event_type`: The SSE event type (e.g., "heartbeat", "share_available")
    /// - `data`: The JSON data for the event
    ///
    /// ### Returns
    /// - `SseEvent`: The parsed event
    pub fn parse(event_type: &str, data: &str) -> Self {
        match event_type {
            "heartbeat" => match serde_json::from_str::<HeartbeatData>(data) {
                Ok(hb) => SseEvent::Heartbeat {
                    timestamp: hb.timestamp,
                },
                Err(e) => {
                    log::warn!("Failed to parse heartbeat: {}", e);
                    SseEvent::Heartbeat {
                        timestamp: String::new(),
                    }
                }
            },
            "share_available" => match serde_json::from_str::<ShareNotification>(data) {
                Ok(notification) => SseEvent::ShareAvailable(notification),
                Err(e) => {
                    log::error!("Failed to parse share notification: {}", e);
                    SseEvent::Error(format!("Invalid share notification: {}", e))
                }
            },
            "" => {
                // No event type means generic message event
                SseEvent::Error(format!("Unknown event (no event type): {}", data))
            }
            _ => {
                log::warn!("Unknown SSE event type: {}", event_type);
                SseEvent::Error(format!("Unknown event type: {}", event_type))
            }
        }
    }
}

impl Fulgur {
    /// Check if the Fulgur is connected to the sync server
    ///
    /// ### Returns
    /// - `true` if Fulgur is connected to the sync server, `false` otherwise
    pub fn is_connected(&self, cx: &App) -> bool {
        self.shared_state(cx)
            .sync_state
            .connection_status
            .lock()
            .is_connected()
    }

    /// Restart the SSE connection with new settings
    ///
    /// ### Description
    /// Stops the current SSE connection and starts a new one with the updated settings.
    /// Should be called when synchronization settings (server URL, email, or key) change.
    pub fn restart_sse_connection(&mut self, cx: &mut Context<Self>) {
        if let Some(ref shutdown_flag) = self.sse_state.sse_shutdown_flag {
            log::info!("Signaling SSE connection to shutdown...");
            shutdown_flag.store(true, Ordering::Relaxed);
        }
        thread::sleep(Duration::from_millis(100));
        let (sse_tx, sse_rx) = std::sync::mpsc::channel();
        let sse_shutdown_flag = Arc::new(AtomicBool::new(false));
        self.sse_state.sse_events = Some(sse_rx);
        self.sse_state.sse_event_tx = Some(sse_tx.clone());
        self.sse_state.sse_shutdown_flag = Some(sse_shutdown_flag.clone());
        if self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            let settings = self.settings.clone();
            let sync_status = self.shared_state(cx).sync_state.connection_status.clone();
            let token_state = Arc::clone(&self.shared_state(cx).sync_state.token_state);
            let http_agent = Arc::clone(&self.shared_state(cx).http_agent);
            thread::spawn(move || {
                // Small delay to ensure old connection is fully stopped
                thread::sleep(Duration::from_millis(200));
                match super::synchronization::initial_synchronization(
                    &settings.app_settings.synchronization_settings,
                    Arc::clone(&token_state),
                    &http_agent,
                ) {
                    Ok(_) => {
                        log::info!("Initial sync succeeded, starting new SSE connection");
                        if let Err(e) = connect_sse(
                            &settings.app_settings.synchronization_settings,
                            sse_tx,
                            sse_shutdown_flag,
                            sync_status,
                            token_state,
                            http_agent,
                        ) {
                            log::error!("Failed to start new SSE connection: {}", e);
                        }
                    }
                    Err(e) => {
                        log::error!("Initial sync failed, not starting SSE: {}", e);
                    }
                }
            });
        } else {
            log::info!("Synchronization is not activated, SSE connection not started");
        }
    }

    /// Handle SSE (Server-Sent Events) from the sync server
    ///
    /// ### Arguments
    /// - `event`: The SSE event to handle
    /// - `window`: The window to show notifications in
    /// - `cx`: The application context
    pub fn handle_sse_event(
        &mut self,
        event: SseEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // Debounce: ignore events within 500ms of last event
        let now = Instant::now();
        if let Some(last_time) = self.sse_state.last_sse_event
            && now.duration_since(last_time) < Duration::from_millis(500)
        {
            return;
        }
        self.sse_state.last_sse_event = Some(now);
        match event {
            SseEvent::Heartbeat { timestamp } => {
                log::debug!("SSE heartbeat received: {}", timestamp);
                let was_disconnected = !self
                    .shared_state(cx)
                    .sync_state
                    .connection_status
                    .lock()
                    .is_connected();
                {
                    let mut last_heartbeat = self.shared_state(cx).sync_state.last_heartbeat.lock();
                    *last_heartbeat = Some(now);
                }
                if was_disconnected {
                    *self.shared_state(cx).sync_state.connection_status.lock() =
                        SynchronizationStatus::Connected;
                    log::info!("Connection restored - heartbeat received after timeout");
                }
            }
            SseEvent::ShareAvailable(notification) => {
                let safe_filename = sanitize_filename(&notification.file_name);
                log::info!(
                    "File shared from device {}: {} (sanitized: {})",
                    notification.source_device_id,
                    notification.file_name,
                    safe_filename
                );
                {
                    let mut files = self.shared_state(cx).sync_state.pending_shared_files.lock();
                    let shared_file = fulgur_common::api::shares::SharedFileResponse {
                        id: notification.share_id,
                        source_device_id: notification.source_device_id.clone(),
                        file_name: safe_filename.clone(),
                        file_size: notification.file_size as i32,
                        content: notification.content,
                        created_at: notification.created_at,
                        expires_at: notification.expires_at,
                    };
                    files.push(shared_file);
                }
                let message = SharedString::from(format!("New file received: {}", safe_filename));
                window.push_notification((NotificationType::Info, message), cx);
            }
            SseEvent::Error(err) => {
                log::error!("SSE error: {}", err);
            }
        }
    }
}
