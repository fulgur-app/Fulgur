use gpui::App;
use std::{
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::{Receiver, Sender},
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
        access_token::{TokenStateManager, get_valid_token},
        share::MAX_SYNC_SHARE_PAYLOAD_BYTES,
        synchronization::{
            SynchronizationError, SynchronizationStatus, set_sync_server_connection_status,
        },
    },
    utils::{retry::BackoffCalculator, sanitize::sanitize_filename, utilities::collect_events},
};

/// Maximum size for SSE event data accumulation (10x payload limit to account for
/// base64 encoding overhead and JSON wrapper)
const MAX_SSE_EVENT_DATA_BYTES: usize = MAX_SYNC_SHARE_PAYLOAD_BYTES * 10;

/// Maximum time to wait for the previous SSE thread to exit before starting a new one
const SSE_THREAD_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Server-Sent Events state for sync functionality
pub struct SseState {
    pub sse_events: Option<Receiver<SseEvent>>,
    pub sse_event_tx: Option<std::sync::mpsc::Sender<SseEvent>>,
    pub sse_shutdown_flag: Option<Arc<AtomicBool>>,
    pub last_sse_event: Option<Instant>,
    /// Handle to the current SSE background thread for lifecycle tracking.
    pub sse_thread_handle: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
}

impl Default for SseState {
    /// Create a new SseState with all fields initialized to default/empty values
    ///
    /// ### Returns
    /// `Self`: A new SseState
    fn default() -> Self {
        Self::new()
    }
}

impl SseState {
    /// Create a new SseState with all fields initialized to None
    ///
    /// ### Returns
    /// `Self`: a new SseState
    pub fn new() -> Self {
        Self {
            sse_events: None,
            sse_event_tx: None,
            sse_shutdown_flag: None,
            last_sse_event: None,
            sse_thread_handle: Arc::new(Mutex::new(None)),
        }
    }
}

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
    token_state: Arc<TokenStateManager>,
    http_agent: Arc<ureq::Agent>,
) -> Result<thread::JoinHandle<()>, SynchronizationError> {
    let server_url = synchronization_settings
        .server_url
        .clone()
        .ok_or(SynchronizationError::ServerUrlMissing)?;
    let sse_url = format!("{}/api/sse", server_url);
    let settings_clone = synchronization_settings.clone();
    let token_state_clone = Arc::clone(&token_state);
    let http_agent_clone = Arc::clone(&http_agent);
    let handle = thread::spawn(move || {
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
                            let fragment = line.trim_start_matches("data:").trim();
                            if current_data.len() + fragment.len() > MAX_SSE_EVENT_DATA_BYTES {
                                log::warn!(
                                    "SSE event data exceeds size limit ({} bytes), discarding",
                                    MAX_SSE_EVENT_DATA_BYTES
                                );
                                current_data.clear();
                                current_event_type.clear();
                                continue;
                            }
                            current_data.push_str(fragment);
                        } else if line.is_empty() && !current_data.is_empty() {
                            log::info!("SSE event type: {}", current_event_type);
                            log::debug!("SSE event received ({} bytes)", current_data.len());
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
    Ok(handle)
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
                log::error!("Unknown event with no event type");
                SseEvent::Error("Unknown event with no event type".to_string())
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
    /// Waits for the previous SSE thread to exit (bounded by `SSE_THREAD_SHUTDOWN_TIMEOUT`)
    /// in a background thread to avoid blocking the UI.
    ///
    /// Should be called when synchronization settings (server URL, email, or key) change.
    ///
    /// ### Arguments
    /// - `cx`: The context of the application.
    pub fn restart_sse_connection(&mut self, cx: &mut Context<Self>) {
        if let Some(ref shutdown_flag) = self.sse_state.sse_shutdown_flag {
            log::info!("Signaling SSE connection to shutdown...");
            shutdown_flag.store(true, Ordering::Relaxed);
        }
        let old_handle = self.sse_state.sse_thread_handle.lock().take();
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
            let handle_storage = Arc::clone(&self.sse_state.sse_thread_handle);
            thread::spawn(move || {
                // Wait for previous SSE thread to exit before starting new connection
                if let Some(handle) = old_handle {
                    let deadline = Instant::now() + SSE_THREAD_SHUTDOWN_TIMEOUT;
                    while !handle.is_finished() && Instant::now() < deadline {
                        thread::sleep(Duration::from_millis(100));
                    }
                    if handle.is_finished() {
                        let _ = handle.join();
                        log::info!("Previous SSE thread exited");
                    } else {
                        log::warn!(
                            "Previous SSE thread still running after {:?}, proceeding with new connection",
                            SSE_THREAD_SHUTDOWN_TIMEOUT
                        );
                    }
                }
                // Small delay to ensure old connection is fully stopped
                thread::sleep(Duration::from_millis(200));
                match super::synchronization::initial_synchronization(
                    &settings.app_settings.synchronization_settings,
                    Arc::clone(&token_state),
                    &http_agent,
                ) {
                    Ok(_) => {
                        log::info!("Initial sync succeeded, starting new SSE connection");
                        match connect_sse(
                            &settings.app_settings.synchronization_settings,
                            sse_tx,
                            sse_shutdown_flag,
                            sync_status,
                            token_state,
                            http_agent,
                        ) {
                            Ok(new_handle) => {
                                *handle_storage.lock() = Some(new_handle);
                            }
                            Err(e) => {
                                log::error!("Failed to start new SSE connection: {}", e);
                            }
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
                if notification.content.len() > MAX_SYNC_SHARE_PAYLOAD_BYTES {
                    log::warn!(
                        "Dropping shared file '{}' from device {}: encrypted payload exceeds {} bytes",
                        notification.file_name,
                        notification.source_device_id,
                        MAX_SYNC_SHARE_PAYLOAD_BYTES
                    );
                    return;
                }
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

    /// Collect and process Server-Sent Events from the sync server:
    /// - Heartbeat: Periodic keepalive messages to detect connection timeouts
    /// - ShareAvailable: Another device has shared a file (triggers file download and decryption)
    /// - Error: Connection or server errors (updates connection status in UI)
    ///
    /// ### Arguments
    /// - `window`: The window to handle events in
    /// - `cx`: The application context
    pub fn process_sse_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let sse_events = collect_events(&self.sse_state.sse_events);
        for event in sse_events {
            self.handle_sse_event(event, window, cx);
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{ShareNotification, SseEvent, SseState};
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState,
        sync::share::MAX_SYNC_SHARE_PAYLOAD_BYTES, sync::synchronization::SynchronizationStatus,
        window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    /// Initialize globals and open a test window with a `gpui_component::Root`-mounted `Fulgur`.
    ///
    /// The root must be a `gpui_component::Root` (not a bare `EmptyView`) because
    /// `window.push_notification(...)` asserts that the first layer is a Root.
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(Default::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    /// Build a minimal valid `ShareNotification` for use in tests.
    fn make_share_notification(file_name: &str, content: &str) -> ShareNotification {
        ShareNotification {
            share_id: "share-123".to_string(),
            source_device_id: "device-src".to_string(),
            destination_device_id: "device-dest".to_string(),
            file_name: file_name.to_string(),
            file_size: content.len() as i64,
            file_hash: "abc123hash".to_string(),
            content: content.to_string(),
            created_at: "2024-01-15T12:00:00Z".to_string(),
            expires_at: "2024-01-16T12:00:00Z".to_string(),
        }
    }

    // --- SseState construction (no GPUI context needed) ---

    #[test]
    fn test_sse_state_new_is_fully_empty() {
        let state = SseState::new();
        assert!(state.sse_events.is_none());
        assert!(state.sse_event_tx.is_none());
        assert!(state.sse_shutdown_flag.is_none());
        assert!(state.last_sse_event.is_none());
        assert!(state.sse_thread_handle.lock().is_none());
    }

    // --- handle_sse_event: Heartbeat ---

    #[gpui::test]
    fn test_handle_heartbeat_sets_last_heartbeat(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_none(),
                    "last_heartbeat should start as None"
                );
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "2024-01-01T00:00:00Z".to_string(),
                    },
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_some(),
                    "last_heartbeat must be set after a heartbeat event"
                );
            });
        });
    }

    #[gpui::test]
    fn test_handle_heartbeat_when_disconnected_restores_connected_status(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Disconnected;
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts".to_string(),
                    },
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .connection_status
                        .lock()
                        .is_connected(),
                    "Heartbeat while Disconnected must restore Connected status"
                );
            });
        });
    }

    #[gpui::test]
    fn test_handle_heartbeat_when_connected_keeps_connected_status(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Connected;
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts".to_string(),
                    },
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .connection_status
                        .lock()
                        .is_connected(),
                    "Heartbeat while already Connected must keep Connected status"
                );
            });
        });
    }

    // --- handle_sse_event: debounce ---

    #[gpui::test]
    fn test_handle_sse_event_debounce_ignores_rapid_second_event(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                // First heartbeat is processed and sets last_sse_event.
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts1".to_string(),
                    },
                    window,
                    cx,
                );
                // Force Disconnected so we can detect whether the second heartbeat
                // is processed (it would restore Connected if not debounced).
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Disconnected;
                // Second heartbeat arrives immediately — within the 500ms debounce window.
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts2".to_string(),
                    },
                    window,
                    cx,
                );
                // Status must still be Disconnected: the rapid second event was debounced.
                assert!(
                    !this
                        .shared_state(cx)
                        .sync_state
                        .connection_status
                        .lock()
                        .is_connected(),
                    "Second event within the 500ms debounce window must be ignored"
                );
            });
        });
    }

    // --- handle_sse_event: ShareAvailable ---

    #[gpui::test]
    fn test_handle_share_available_adds_file_to_pending_files(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_shared_files
                        .lock()
                        .is_empty(),
                    "pending_shared_files should start empty"
                );
                let notification = make_share_notification("report.txt", "file_content");
                this.handle_sse_event(SseEvent::ShareAvailable(notification), window, cx);
                let files = this.shared_state(cx).sync_state.pending_shared_files.lock();
                assert_eq!(files.len(), 1, "exactly one file must be added");
                assert_eq!(files[0].file_name, "report.txt");
            });
        });
    }

    #[gpui::test]
    fn test_handle_share_available_oversized_payload_is_dropped(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let oversized_content = "x".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES + 1);
                let notification = make_share_notification("huge.bin", &oversized_content);
                this.handle_sse_event(SseEvent::ShareAvailable(notification), window, cx);
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_shared_files
                        .lock()
                        .is_empty(),
                    "Oversized payload must be silently dropped"
                );
            });
        });
    }

    // --- handle_sse_event: Error ---

    #[gpui::test]
    fn test_handle_error_event_does_not_change_shared_state(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_none()
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_shared_files
                        .lock()
                        .is_empty()
                );
                // Must not panic and must not alter heartbeat or shared files.
                this.handle_sse_event(
                    SseEvent::Error("connection timeout".to_string()),
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_none()
                );
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .pending_shared_files
                        .lock()
                        .is_empty()
                );
            });
        });
    }

    // --- process_sse_events ---

    #[gpui::test]
    fn test_process_sse_events_dispatches_heartbeat_from_channel(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tx, rx) = std::sync::mpsc::channel();
                this.sse_state.sse_events = Some(rx);
                tx.send(SseEvent::Heartbeat {
                    timestamp: "ts".to_string(),
                })
                .unwrap();
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_none()
                );
                this.process_sse_events(window, cx);
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_some(),
                    "Heartbeat from channel must be dispatched by process_sse_events"
                );
            });
        });
    }

    #[gpui::test]
    fn test_process_sse_events_with_empty_channel_is_a_no_op(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (_tx, rx) = std::sync::mpsc::channel::<SseEvent>();
                this.sse_state.sse_events = Some(rx);
                this.process_sse_events(window, cx);
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_none(),
                    "No events in channel means no heartbeat should be set"
                );
            });
        });
    }

    #[gpui::test]
    fn test_process_sse_events_with_closed_channel_is_a_no_op(cx: &mut TestAppContext) {
        // Fulgur::new always creates a channel, so sse_events is never None after
        // construction. Replace it with a receiver whose sender has been dropped
        // (closed channel) to verify process_sse_events handles EOF gracefully.
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let (tx, rx) = std::sync::mpsc::channel::<SseEvent>();
                this.sse_state.sse_events = Some(rx);
                drop(tx); // close the channel immediately
                this.process_sse_events(window, cx); // must not panic
                assert!(
                    this.shared_state(cx)
                        .sync_state
                        .last_heartbeat
                        .lock()
                        .is_none(),
                    "No events dispatched from closed channel"
                );
            });
        });
    }
}
