use super::access_token::{TokenStateManager, get_valid_token};
use super::sse::connect_sse;
use crate::fulgur::Fulgur;
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::shared_state::SyncState;
use crate::fulgur::sync::share;
use crate::fulgur::ui::tabs::editor_tab;
use crate::fulgur::ui::tabs::tab::Tab;
use crate::fulgur::utils::crypto_helper::{
    self, load_device_api_key_from_keychain, load_private_key_from_keychain,
};
use crate::fulgur::utils::sanitize::sanitize_filename;
use fulgur_common::api::sync::{
    BeginResponse, BeginV2Response, InitialSynchronizationPayload, PingResponse,
};
use gpui::{App, Context, SharedString, Window};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::{
    fmt,
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use crate::fulgur::ui::notifications::progress::{CancelCallback, start_progress};

/// Maximum number of bytes accepted from small JSON HTTP responses (token, ping).
pub const MAX_HTTP_SMALL_RESPONSE_BYTES: u64 = 64 * 1024;

/// Top-level JSON framing overhead allowance for a single-share response
/// (object braces, sibling fields like `id`, `file_name`, timestamps).
const SHARE_RESPONSE_FRAMING_BYTES: u64 = 4 * 1024;

/// Maximum number of bytes accepted from a single-share HTTP response
/// (`GET /api/shares/:id`).
pub const MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES: u64 = (share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64)
    + (share::JSON_OVERHEAD_PER_SHARE_BYTES as u64)
    + SHARE_RESPONSE_FRAMING_BYTES;

/// Maximum number of bytes accepted from the legacy `POST /api/begin` response,
/// which inlines every pending share. Sized to fit the worst-case bundle:
/// `MAX_PENDING_SHARES_PER_RESPONSE` shares each at the single-share cap.
const MAX_HTTP_V1_BEGIN_RESPONSE_BYTES: u64 = (share::MAX_PENDING_SHARES_PER_RESPONSE as u64)
    * ((share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64)
        + (share::JSON_OVERHEAD_PER_SHARE_BYTES as u64))
    + SHARE_RESPONSE_FRAMING_BYTES;

/// Handle ureq errors and convert them to `SynchronizationError` with appropriate logging
///
/// ### Description
/// Centralizes ureq error handling logic that was duplicated across sync modules.
/// Maps all ureq error variants to appropriate `SynchronizationError` types and logs
/// the error with context.
///
/// ### Arguments
/// - `error`: The ureq error to handle
/// - `context`: Human-readable context for logging (e.g., "Failed to get devices")
///
/// ### Returns
/// - `SynchronizationError`: The mapped synchronization error
pub fn handle_ureq_error(error: ureq::Error, context: &str) -> SynchronizationError {
    match error {
        ureq::Error::StatusCode(code) => {
            log::error!("{context}: HTTP status {code}");
            if code == 401 || code == 403 {
                SynchronizationError::AuthenticationFailed
            } else if code == 400 {
                SynchronizationError::BadRequest
            } else if code == 413 {
                SynchronizationError::ContentTooLarge {
                    file_size: 0,
                    max_size: 0,
                }
            } else {
                SynchronizationError::ServerError(code)
            }
        }
        ureq::Error::Io(io_error) => {
            log::error!("{context} (IO): {io_error}");
            match io_error.kind() {
                std::io::ErrorKind::ConnectionRefused => SynchronizationError::ConnectionFailed,
                std::io::ErrorKind::TimedOut => SynchronizationError::Timeout(io_error.to_string()),
                std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::ConnectionAborted => {
                    SynchronizationError::ConnectionFailed
                }
                std::io::ErrorKind::AddrNotAvailable => SynchronizationError::HostNotFound,
                _ => SynchronizationError::Other(io_error.to_string()),
            }
        }
        ureq::Error::ConnectionFailed => {
            log::error!("{context}: Connection failed");
            SynchronizationError::ConnectionFailed
        }
        ureq::Error::HostNotFound => {
            log::error!("{context}: Host not found");
            SynchronizationError::HostNotFound
        }
        ureq::Error::Timeout(timeout) => {
            log::error!("{context}: Timeout ({timeout})");
            SynchronizationError::Timeout(timeout.to_string())
        }
        e => {
            log::error!("{context}: {e}");
            SynchronizationError::Other(e.to_string())
        }
    }
}

/// Validate and persist the server-advertised `max_file_size_bytes`.
///
/// ### Arguments
/// - `atomic`: The shared atomic holding the current cap
/// - `advertised`: The `Option<u64>` received from the server's response
pub fn store_server_max_file_size(atomic: &std::sync::atomic::AtomicU64, advertised: Option<u64>) {
    let value = match advertised {
        None => {
            log::info!("Server max file size: no limit");
            u64::MAX
        }
        Some(0) => {
            log::warn!(
                "Server advertised max_file_size_bytes = 0 (would disable sharing); falling back to {} bytes",
                share::MAX_SYNC_SHARE_PAYLOAD_BYTES
            );
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        }
        Some(n) => {
            log::info!("Server max file size: {n} bytes");
            n
        }
    };
    atomic.store(value, std::sync::atomic::Ordering::Release);
}

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
///
/// ### Returns
/// - `Ok(BeginResponse)`: Device name, max file size, and pending shares
/// - `Err(SynchronizationError)`: If both v2 and v1 begin calls failed
pub fn initial_synchronization(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<BeginResponse, SynchronizationError> {
    match initial_synchronization_v2(profile, token_state, http_agent) {
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

/// Initial synchronization via the v2 begin flow.
///
/// ### Description
/// Calls `POST /api/v2/begin` to update the device's encryption key and obtain
/// the list of pending share IDs, then fetches each share individually via
/// `GET /api/shares/:id`.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with
/// - `token_state`: Per-profile JWT token state manager
/// - `http_agent`: Shared HTTP agent for connection pooling
///
/// ### Returns
/// - `Ok(BeginResponse)`: Device name, max file size, and successfully fetched pending shares
/// - `Err(SynchronizationError)`: If the v2 begin call failed or returned an invalid response
fn initial_synchronization_v2(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<BeginResponse, SynchronizationError> {
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
    let shares: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = begin_v2
            .share_ids
            .iter()
            .map(|id| {
                scope.spawn(move || {
                    (
                        id.as_str(),
                        share::fetch_share_by_id(profile, token_state, http_agent, id),
                    )
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
    });
    log::info!(
        "Initial synchronization (v2) successful: {} announced, {} retrieved",
        begin_v2.share_ids.len(),
        shares.len()
    );
    Ok(BeginResponse {
        device_name: begin_v2.device_name,
        shares,
        max_file_size_bytes: begin_v2.max_file_size_bytes,
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
/// - `Ok(BeginResponse)`: Device name, max file size, and pending shares
/// - `Err(SynchronizationError)`: If the v1 begin call failed or returned an invalid response
fn initial_synchronization_v1(
    profile: &ServerProfile,
    token_state: &Arc<TokenStateManager>,
    http_agent: &ureq::Agent,
) -> Result<BeginResponse, SynchronizationError> {
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
    let begin_response: BeginResponse = match serde_json::from_str(&body) {
        Ok(response) => response,
        Err(e) => {
            log::error!("Failed to parse v1 begin response body: {e}");
            return Err(SynchronizationError::InvalidResponse(e.to_string()));
        }
    };
    log::info!(
        "Initial synchronization (v1) successful with {} shared files",
        begin_response.shares.len()
    );
    Ok(begin_response)
}

/// Ping an authenticated Fulgurant server endpoint to test connectivity and credentials.
///
/// ### Arguments
/// - `server_url`: The base URL of the server (e.g. `https://example.com`).
/// - `token`: A valid JWT Bearer token.
/// - `http_agent`: Shared HTTP agent.
///
/// ### Returns
/// - `Ok(())`: Server responded with `ok: true`.
/// - `Err(SynchronizationError)`: Server is unreachable, auth failed, or returned an unexpected response.
pub fn ping_server(
    server_url: &str,
    token: &str,
    http_agent: &ureq::Agent,
) -> Result<(), SynchronizationError> {
    let ping_url = format!("{server_url}/api/ping");
    let mut response = http_agent
        .get(&ping_url)
        .header("Authorization", &format!("Bearer {token}"))
        .call()
        .map_err(|e| handle_ureq_error(e, "Ping failed"))?;
    let body = response
        .body_mut()
        .with_config()
        .limit(MAX_HTTP_SMALL_RESPONSE_BYTES)
        .read_to_string()
        .map_err(|e| SynchronizationError::Other(e.to_string()))?;
    let ping_response: PingResponse = serde_json::from_str(&body)
        .map_err(|e| SynchronizationError::InvalidResponse(e.to_string()))?;
    if ping_response.ok {
        Ok(())
    } else {
        Err(SynchronizationError::Other(
            "Server returned ok: false".to_string(),
        ))
    }
}

/// Ping a Fulgurant server with a progress indicator and a result notification.
///
/// ### Arguments
/// - `profile`: The server profile to authenticate and ping.
/// - `display_name`: Human-readable label shown in the progress/result notifications.
/// - `window`: The window to attach the progress indicator to.
/// - `cx`: The application context.
pub fn perform_ping_with_progress(
    profile: ServerProfile,
    display_name: String,
    window: &mut Window,
    cx: &mut App,
) {
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_state = shared.sync_state_for(&profile.id);
    let pending_notification = sync_state.pending_notification.clone();
    let token_state = Arc::clone(&sync_state.token_state);
    let http_agent = Arc::clone(&shared.http_agent);

    let done = Arc::new(AtomicBool::new(false));
    let done_for_thread = Arc::clone(&done);

    let progress = start_progress(
        window,
        cx,
        format!("Testing connection to {display_name}...").into(),
        None,
    );

    thread::spawn(move || {
        let result = get_valid_token(&profile, &token_state, &http_agent).and_then(|token| {
            match profile.server_url.as_deref() {
                Some(url) => ping_server(url, &token, &http_agent),
                None => Err(SynchronizationError::ServerUrlMissing),
            }
        });
        let notification = match result {
            Ok(()) => (
                NotificationType::Success,
                SharedString::from(format!("{display_name}: Server is reachable")),
            ),
            Err(e) => (
                NotificationType::Error,
                SharedString::from(format!("{display_name}: Ping failed: {e}")),
            ),
        };
        *pending_notification.lock() = Some(notification);
        done_for_thread.store(true, Ordering::Release);
    });

    window
        .spawn(cx, async move |async_cx| {
            let _progress = progress;
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if done.load(Ordering::Acquire) {
                    break;
                }
            }
        })
        .detach();
}

/// Fetches shared files from each active profile's server and starts SSE
/// connections for real-time updates.
///
/// ### Arguments
/// - `entity`: The Fulgur entity
/// - `cx`: The application context.
pub fn begin_synchronization(entity: &gpui::Entity<crate::fulgur::Fulgur>, cx: &gpui::App) {
    if !entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .is_synchronization_activated
    {
        return;
    }
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    if shared
        .sync_initialized
        .swap(true, std::sync::atomic::Ordering::SeqCst)
    {
        log::debug!("Sync already initialized by another window");
        return;
    }
    log::info!("Initializing sync system");
    let active_profiles: Vec<ServerProfile> = entity
        .read(cx)
        .settings
        .app_settings
        .synchronization_settings
        .profiles
        .iter()
        .filter(|p| p.is_active)
        .cloned()
        .collect();
    let http_agent = Arc::clone(&shared.http_agent);
    for profile in active_profiles {
        let sync_state = shared.sync_state_for(&profile.id);
        let sse_tx = entity
            .read(cx)
            .sse_states
            .get(&profile.id)
            .and_then(|s| s.sse_event_tx.clone());
        let sse_shutdown_flag = entity
            .read(cx)
            .sse_states
            .get(&profile.id)
            .and_then(|s| s.sse_shutdown_flag.clone());
        let sse_thread_handle = entity
            .read(cx)
            .sse_states
            .get(&profile.id)
            .map(|s| s.sse_thread_handle.clone());
        let http_agent_clone = Arc::clone(&http_agent);
        thread::spawn(move || {
            run_profile_bootstrap(
                &profile,
                &sync_state,
                sse_tx,
                sse_shutdown_flag,
                sse_thread_handle,
                &http_agent_clone,
            );
        });
    }
}

/// Run the bootstrap sequence for a single profile in a background thread.
///
/// ### Arguments
/// - `profile`: The profile being bootstrapped.
/// - `sync_state`: Shared per-profile sync state.
/// - `sse_tx`: Optional SSE event sender; `None` skips the SSE step.
/// - `sse_shutdown_flag`: Shutdown flag signalled by `restart_sse_connection`.
/// - `sse_thread_handle`: Slot for the SSE worker thread handle.
/// - `http_agent`: Shared HTTP agent.
fn run_profile_bootstrap(
    profile: &ServerProfile,
    sync_state: &Arc<SyncState>,
    sse_tx: Option<std::sync::mpsc::Sender<crate::fulgur::sync::sse::SseEvent>>,
    sse_shutdown_flag: Option<Arc<AtomicBool>>,
    sse_thread_handle: Option<Arc<Mutex<Option<thread::JoinHandle<()>>>>>,
    http_agent: &Arc<ureq::Agent>,
) {
    // Small delay to ensure app initialization doesn't block
    thread::sleep(Duration::from_millis(100));
    let key = match load_device_api_key_from_keychain(&profile.id) {
        Ok(value) => value,
        Err(e) => {
            log::error!(
                "Profile '{}': failed to load device API key from keychain: {e}",
                profile.name
            );
            set_sync_server_connection_status(
                &sync_state.connection_status,
                SynchronizationStatus::Disconnected,
            );
            return;
        }
    };
    if profile.server_url.is_none() || profile.email.is_none() || key.is_none() {
        set_sync_server_connection_status(
            &sync_state.connection_status,
            SynchronizationStatus::Disconnected,
        );
        return;
    }
    match initial_synchronization(profile, &sync_state.token_state, http_agent) {
        Ok(begin_response) => {
            log::info!("Profile '{}': connected to sync server", profile.name);
            set_sync_server_connection_status(
                &sync_state.connection_status,
                SynchronizationStatus::Connected,
            );
            store_server_max_file_size(
                &sync_state.max_file_size_bytes,
                begin_response.max_file_size_bytes,
            );
            {
                let mut device_name = sync_state.device_name.lock();
                *device_name = Some(begin_response.device_name);
            }
            {
                let mut files = sync_state.pending_shared_files.lock();
                *files = begin_response
                    .shares
                    .into_iter()
                    .map(|mut share| {
                        share.file_name = sanitize_filename(&share.file_name);
                        share
                    })
                    .collect();
            }
            if let (Some(tx), Some(shutdown), Some(handle_storage)) =
                (sse_tx, sse_shutdown_flag, sse_thread_handle)
            {
                log::info!(
                    "Profile '{}': starting SSE connection for real-time updates",
                    profile.name
                );
                match connect_sse(
                    profile,
                    tx,
                    shutdown,
                    sync_state.connection_status.clone(),
                    &sync_state.token_state,
                    http_agent,
                    &sync_state.pending_shared_files,
                ) {
                    Ok(handle) => {
                        *handle_storage.lock() = Some(handle);
                    }
                    Err(e) => {
                        log::error!("Profile '{}': failed to start SSE: {e}", profile.name);
                    }
                }
            } else {
                log::warn!(
                    "Profile '{}': SSE channels not available, skipping SSE start",
                    profile.name
                );
            }
        }
        Err(e) => {
            log::error!(
                "Profile '{}': initial synchronization failed: {e}",
                profile.name
            );
            set_sync_server_connection_status(
                &sync_state.connection_status,
                SynchronizationStatus::Disconnected,
            );
        }
    }
}

/// Set the synchronization status of the sync server
///
/// ### Arguments
/// - `sync_server_connection_status`: The synchronization status of the sync server
/// - `status`: The new synchronization status
pub fn set_sync_server_connection_status(
    sync_server_connection_status: &Arc<Mutex<SynchronizationStatus>>,
    new_status: SynchronizationStatus,
) {
    *sync_server_connection_status.lock() = new_status;
}

/// Perform initial synchronization with a single profile's server in a
/// background thread.
///
/// Sets the connection status to `Connecting` immediately, then spawns a background
/// thread to perform the actual network call. The UI remains responsive while the
/// connection is in progress.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with.
/// - `cx`: The application context (used to obtain shared state).
pub fn perform_initial_synchronization(profile: ServerProfile, cx: &mut App) {
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_state = shared.sync_state_for(&profile.id);
    set_sync_server_connection_status(
        &sync_state.connection_status,
        SynchronizationStatus::Connecting,
    );
    *sync_state.connecting_since.lock() = Some(Instant::now());
    let token_state = Arc::clone(&sync_state.token_state);
    let http_agent = Arc::clone(&shared.http_agent);
    let profile_name = profile.name.clone();
    let connection_status = sync_state.connection_status.clone();
    let connecting_since = sync_state.connecting_since.clone();
    let device_name = sync_state.device_name.clone();
    let pending_shared_files = sync_state.pending_shared_files.clone();
    let pending_notification = sync_state.pending_notification.clone();
    let max_file_size_bytes = sync_state.max_file_size_bytes.clone();
    thread::spawn(move || {
        let result = initial_synchronization(&profile, &token_state, &http_agent);
        let (notification, status) = match result {
            Ok(begin_response) => {
                store_server_max_file_size(
                    &max_file_size_bytes,
                    begin_response.max_file_size_bytes,
                );
                {
                    let mut name = device_name.lock();
                    *name = Some(begin_response.device_name.clone());
                }
                {
                    let mut files = pending_shared_files.lock();
                    *files = begin_response.shares;
                }
                (
                    (
                        NotificationType::Success,
                        SharedString::from(format!(
                            "{profile_name}: Connection successful as {}",
                            begin_response.device_name
                        )),
                    ),
                    SynchronizationStatus::Connected,
                )
            }
            Err(e) => (
                (
                    NotificationType::Error,
                    SharedString::from(format!("{profile_name}: Connection failed: {e}")),
                ),
                SynchronizationStatus::from_error(&e),
            ),
        };
        set_sync_server_connection_status(&connection_status, status);
        *connecting_since.lock() = None;
        *pending_notification.lock() = Some(notification);
    });
}

/// Perform initial synchronization with a single profile's server, showing
/// a progress spinner.
///
/// ### Arguments
/// - `profile`: The server profile to synchronize with.
/// - `window`: Target window for the progress notification.
/// - `cx`: The application context.
pub fn perform_initial_synchronization_with_progress(
    profile: ServerProfile,
    window: &mut Window,
    cx: &mut App,
) {
    let shared = cx.global::<crate::fulgur::shared_state::SharedAppState>();
    let sync_state = shared.sync_state_for(&profile.id);
    set_sync_server_connection_status(
        &sync_state.connection_status,
        SynchronizationStatus::Connecting,
    );
    *sync_state.connecting_since.lock() = Some(Instant::now());
    let token_state = Arc::clone(&sync_state.token_state);
    let http_agent = Arc::clone(&shared.http_agent);
    let profile_name = profile.name.clone();
    let connection_status = sync_state.connection_status.clone();
    let connecting_since = sync_state.connecting_since.clone();
    let device_name = sync_state.device_name.clone();
    let pending_shared_files = sync_state.pending_shared_files.clone();
    let pending_notification = sync_state.pending_notification.clone();
    let max_file_size_bytes = sync_state.max_file_size_bytes.clone();

    let done = Arc::new(AtomicBool::new(false));
    let done_for_thread = Arc::clone(&done);

    let cancel_status = connection_status.clone();
    let cancel_connecting_since = connecting_since.clone();
    let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, _cx| {
        set_sync_server_connection_status(&cancel_status, SynchronizationStatus::Disconnected);
        *cancel_connecting_since.lock() = None;
    }));

    let progress = start_progress(
        window,
        cx,
        format!("Connecting to {profile_name}...").into(),
        cancel_callback,
    );
    let cancel_flag = progress.cancel_flag();
    let cancel_flag_for_thread = Arc::clone(&cancel_flag);

    thread::spawn(move || {
        let result = initial_synchronization(&profile, &token_state, &http_agent);

        if cancel_flag_for_thread.load(Ordering::Acquire) {
            done_for_thread.store(true, Ordering::Release);
            return;
        }

        let (notification, status) = match result {
            Ok(begin_response) => {
                store_server_max_file_size(
                    &max_file_size_bytes,
                    begin_response.max_file_size_bytes,
                );
                {
                    let mut name = device_name.lock();
                    *name = Some(begin_response.device_name.clone());
                }
                {
                    let mut files = pending_shared_files.lock();
                    *files = begin_response.shares;
                }
                (
                    (
                        NotificationType::Success,
                        SharedString::from(format!(
                            "{profile_name}: Connection successful as {}",
                            begin_response.device_name
                        )),
                    ),
                    SynchronizationStatus::Connected,
                )
            }
            Err(e) => (
                (
                    NotificationType::Error,
                    SharedString::from(format!("{profile_name}: Connection failed: {e}")),
                ),
                SynchronizationStatus::from_error(&e),
            ),
        };
        set_sync_server_connection_status(&connection_status, status);
        *connecting_since.lock() = None;
        *pending_notification.lock() = Some(notification);
        done_for_thread.store(true, Ordering::Release);
    });

    window
        .spawn(cx, async move |async_cx| {
            let _progress = progress;
            loop {
                async_cx
                    .background_executor()
                    .timer(Duration::from_millis(100))
                    .await;
                if done.load(Ordering::Acquire) || cancel_flag.load(Ordering::Acquire) {
                    break;
                }
            }
        })
        .detach();
}

#[derive(Debug)]
pub enum SynchronizationError {
    AuthenticationFailed,
    BadRequest,
    CompressionFailed,
    ConnectionFailed,
    ContentMissing,
    ContentTooLarge { file_size: usize, max_size: usize },
    DeviceIdsMissing,
    DeviceKeyMissing,
    EmailMissing,
    EncryptionFailed,
    FileNameMissing,
    HostNotFound,
    InvalidPublicKey(String),
    InvalidResponse(String),
    MissingEncryptionKey,
    MissingPublicKey(String),
    MissingExpirationDate,
    Other(String),
    ServerError(u16),
    ServerUrlMissing,
    Timeout(String),
}

impl fmt::Display for SynchronizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SynchronizationError::AuthenticationFailed => write!(f, "Authentication failed"),
            SynchronizationError::BadRequest => write!(f, "Bad request"),
            SynchronizationError::CompressionFailed => write!(f, "Compression failed"),
            SynchronizationError::ConnectionFailed => write!(f, "Cannot connect to sync server"),
            SynchronizationError::ContentMissing => write!(f, "Content is missing"),
            SynchronizationError::ContentTooLarge {
                file_size: 0,
                max_size: 0,
            } => write!(f, "File is too large to share (rejected by server)"),
            SynchronizationError::ContentTooLarge {
                file_size,
                max_size,
            } => write!(
                f,
                "File is too large to share ({} KB, max {} KB)",
                file_size / 1024,
                max_size / 1024
            ),
            SynchronizationError::DeviceIdsMissing => write!(f, "Device IDs are missing"),
            SynchronizationError::DeviceKeyMissing => write!(f, "Key is missing"),
            SynchronizationError::EmailMissing => write!(f, "Email is missing"),
            SynchronizationError::EncryptionFailed => write!(f, "Encryption failed"),
            SynchronizationError::FileNameMissing => write!(f, "File name is missing"),
            SynchronizationError::HostNotFound => write!(f, "Host not found"),
            SynchronizationError::InvalidPublicKey(name) => {
                write!(f, "Invalid public key for device: {name}")
            }
            SynchronizationError::InvalidResponse(e) => write!(f, "{e}"),
            SynchronizationError::MissingEncryptionKey => write!(f, "Missing encryption key"),
            SynchronizationError::MissingExpirationDate => write!(f, "Missing expiration date"),
            SynchronizationError::MissingPublicKey(e) => {
                write!(f, "Missing public key for device: {e}")
            }
            SynchronizationError::Other(e) => write!(f, "{e}"),
            SynchronizationError::ServerError(e) => write!(f, "{e}"),
            SynchronizationError::ServerUrlMissing => write!(f, "Server URL is missing"),
            SynchronizationError::Timeout(timeout) => write!(f, "Timeout: {timeout}"),
        }
    }
}

#[derive(Clone, Copy)]
pub enum SynchronizationStatus {
    Connected,
    Connecting,
    Disconnected,
    AuthenticationFailed,
    ConnectionFailed,
    Other,
    NotActivated,
}

impl SynchronizationStatus {
    /// Convert the error to a synchronization status
    ///
    /// ### Arguments
    /// - `error`: The error
    ///
    /// ### Returns
    /// - `SynchronizationStatus`: The synchronization status
    pub fn from_error(error: &SynchronizationError) -> SynchronizationStatus {
        match error {
            SynchronizationError::AuthenticationFailed => {
                SynchronizationStatus::AuthenticationFailed
            }
            SynchronizationError::HostNotFound => SynchronizationStatus::ConnectionFailed,
            SynchronizationError::ConnectionFailed => SynchronizationStatus::ConnectionFailed,
            SynchronizationError::Timeout(_) => SynchronizationStatus::ConnectionFailed,
            _ => SynchronizationStatus::Other,
        }
    }

    /// Check if the synchronization status is connected
    ///
    /// ### Returns
    /// - `true` if the synchronization status is connected, `false` otherwise
    pub fn is_connected(&self) -> bool {
        match self {
            SynchronizationStatus::Connected => true,
            SynchronizationStatus::Connecting => false,
            SynchronizationStatus::Disconnected => false,
            SynchronizationStatus::AuthenticationFailed => false,
            SynchronizationStatus::ConnectionFailed => false,
            SynchronizationStatus::Other => false,
            SynchronizationStatus::NotActivated => false,
        }
    }

    /// Check if the synchronization status is connecting
    ///
    /// ### Returns
    /// - `true` if the synchronization status is connecting, `false` otherwise
    pub fn is_connecting(&self) -> bool {
        matches!(self, SynchronizationStatus::Connecting)
    }

    /// Return a short human-readable label for display in tooltips and status pills.
    ///
    /// ### Returns
    /// - `&'static str`: One of "Connected", "Connecting", "Disconnected", "Inactive", or "Error".
    pub fn label(self) -> &'static str {
        match self {
            SynchronizationStatus::Connected => "Connected",
            SynchronizationStatus::Connecting => "Connecting",
            SynchronizationStatus::Disconnected => "Disconnected",
            SynchronizationStatus::NotActivated => "Inactive",
            SynchronizationStatus::AuthenticationFailed
            | SynchronizationStatus::ConnectionFailed
            | SynchronizationStatus::Other => "Error",
        }
    }
}

impl Fulgur {
    /// Process shared files received from every active sync profile.
    ///
    /// ### Arguments
    /// - `window`: The window to create new tabs in
    /// - `cx`: The application context
    pub fn process_shared_files_from_sync(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile_ids: Vec<crate::fulgur::settings::ProfileId> = self
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .filter(|p| p.is_active)
            .map(|p| p.id.clone())
            .collect();
        for profile_id in profile_ids {
            let sync_state = self.shared_state(cx).sync_state_for(&profile_id);
            let shared_files_to_open =
                if let Some(mut pending) = sync_state.pending_shared_files.try_lock() {
                    if pending.is_empty() {
                        Vec::new()
                    } else {
                        log::info!(
                            "Processing {} shared file(s) for profile {profile_id}",
                            pending.len()
                        );
                        pending.drain(..).collect()
                    }
                } else {
                    Vec::new()
                };
            if shared_files_to_open.is_empty() {
                continue;
            }
            let encryption_key_opt = match load_private_key_from_keychain(&profile_id) {
                Ok(key) => key,
                Err(_) => {
                    log::error!(
                        "Cannot decrypt shared files for profile {profile_id}: encryption key not available"
                    );
                    None
                }
            };
            let server_max_size = sync_state
                .max_file_size_bytes
                .load(std::sync::atomic::Ordering::Acquire);
            if let Some(encryption_key) = encryption_key_opt {
                for shared_file in shared_files_to_open {
                    if server_max_size != u64::MAX
                        && shared_file.content.len() as u64 > server_max_size.saturating_mul(2)
                    {
                        log::warn!(
                            "Skipping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds 2x the server max ({} bytes)",
                            shared_file.file_name,
                            shared_file.source_device_id,
                            shared_file.content.len(),
                            server_max_size
                        );
                        continue;
                    }
                    let decrypted_result =
                        crypto_helper::decrypt_bytes(&shared_file.content, &encryption_key)
                            .and_then(|compressed_bytes| {
                                share::decompress_content(&compressed_bytes, server_max_size)
                            });
                    match decrypted_result {
                        Ok(decrypted_content) => {
                            let tab_id = self.next_tab_id;
                            self.next_tab_id += 1;
                            let new_tab = Tab::Editor(editor_tab::EditorTab::from_content(
                                tab_id,
                                &decrypted_content,
                                shared_file.file_name.clone(),
                                window,
                                cx,
                                &self.settings.editor_settings,
                            ));
                            self.tabs.push(new_tab);
                            self.active_tab_index = Some(self.tabs.len() - 1);
                            self.pending_tab_scroll = Some(self.tabs.len() - 1);
                            log::info!("Opened shared file: {}", shared_file.file_name);
                        }
                        Err(e) => {
                            log::error!(
                                "Failed to decrypt shared file {}: {}",
                                shared_file.file_name,
                                e
                            );
                        }
                    }
                }
            } else {
                log::error!("Cannot decrypt shared files: encryption key not available");
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::store_server_max_file_size;
    use crate::fulgur::sync::share;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn none_is_stored_as_unlimited() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, None);
        assert_eq!(atomic.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn zero_is_replaced_with_safe_default() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(0));
        assert_eq!(
            atomic.load(Ordering::Acquire),
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        );
    }

    #[test]
    fn positive_values_are_accepted_verbatim() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(5 * 1024 * 1024));
        assert_eq!(atomic.load(Ordering::Acquire), 5 * 1024 * 1024);
    }

    #[test]
    fn very_large_values_are_trusted_as_user_choice() {
        let atomic = AtomicU64::new(0);
        store_server_max_file_size(&atomic, Some(u64::MAX - 1));
        assert_eq!(atomic.load(Ordering::Acquire), u64::MAX - 1);
    }
}
