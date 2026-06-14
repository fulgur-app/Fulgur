use super::access_token::{TokenStateManager, get_valid_token};
use super::sse::{SseAgents, SseShareState, connect_sse};
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

/// Maximum number of bytes accepted from the devices listing response
/// (`GET /api/devices`).
pub const MAX_HTTP_DEVICES_RESPONSE_BYTES: u64 = 1024 * 1024;

/// Top-level JSON framing overhead allowance for a single-share response
/// (object braces, sibling fields like `id`, `file_name`, timestamps).
const SHARE_RESPONSE_FRAMING_BYTES: u64 = 4 * 1024;

/// Maximum number of bytes accepted from a single-share HTTP response
/// (`GET /api/shares/:id`) when the server advertises no size limit.
pub const MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES: u64 = (share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64)
    * 2
    + (share::JSON_OVERHEAD_PER_SHARE_BYTES as u64)
    + SHARE_RESPONSE_FRAMING_BYTES;

/// Maximum number of bytes accepted from the legacy `POST /api/begin` response. Sized to fit the worst-case bundle.
const MAX_HTTP_V1_BEGIN_RESPONSE_BYTES: u64 = (share::MAX_PENDING_SHARES_PER_RESPONSE as u64)
    * ((share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64)
        + (share::JSON_OVERHEAD_PER_SHARE_BYTES as u64))
    + SHARE_RESPONSE_FRAMING_BYTES;

/// Compute the wire-size cap for a bulk `GET /api/shares` drain, derived from
/// the server's advertised maximum file size.
///
/// ### Arguments
/// - `server_max_file_size`: The server-advertised max file size in bytes, or
///   `u64::MAX` when the server reports no limit.
///
/// ### Returns
/// - `u64`: The maximum number of bytes to accept from the bulk drain response.
pub fn max_http_bulk_shares_response_bytes(server_max_file_size: u64) -> u64 {
    if server_max_file_size == u64::MAX {
        return MAX_HTTP_V1_BEGIN_RESPONSE_BYTES;
    }
    let per_share_wire = server_max_file_size
        .saturating_mul(2)
        .saturating_add(share::JSON_OVERHEAD_PER_SHARE_BYTES as u64);
    (share::MAX_PENDING_SHARES_PER_RESPONSE as u64)
        .saturating_mul(per_share_wire)
        .saturating_add(SHARE_RESPONSE_FRAMING_BYTES)
}

/// Compute the wire-size cap for a single `GET /api/shares/:id` fetch, derived from the server's advertised maximum file size.
///
/// ### Arguments
/// - `server_max_file_size`: The server-advertised max file size in bytes, or
///   `u64::MAX` when the server reports no limit.
///
/// ### Returns
/// - `u64`: The maximum number of bytes to accept from the single-share response.
pub fn max_http_single_share_response_bytes(server_max_file_size: u64) -> u64 {
    if server_max_file_size == u64::MAX {
        return MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES;
    }
    server_max_file_size
        .saturating_mul(2)
        .saturating_add(share::JSON_OVERHEAD_PER_SHARE_BYTES as u64)
        .saturating_add(SHARE_RESPONSE_FRAMING_BYTES)
}

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

/// Resolve the server-advertised `max_file_size_bytes` into a concrete cap.
///
/// ### Arguments
/// - `advertised`: The `Option<u64>` received from the server's response
///
/// ### Returns
/// - `u64`: The resolved cap in bytes, or `u64::MAX` when unlimited
pub fn resolve_server_max_file_size(advertised: Option<u64>) -> u64 {
    match advertised {
        None => u64::MAX,
        Some(0) => share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
        Some(n) => n,
    }
}

/// Validate and persist the server-advertised `max_file_size_bytes`.
///
/// ### Arguments
/// - `atomic`: The shared atomic holding the current cap
/// - `advertised`: The `Option<u64>` received from the server's response
pub fn store_server_max_file_size(atomic: &std::sync::atomic::AtomicU64, advertised: Option<u64>) {
    match advertised {
        None => log::info!("Server max file size: no limit"),
        Some(0) => log::warn!(
            "Server advertised max_file_size_bytes = 0 (would disable sharing); falling back to {} bytes",
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES
        ),
        Some(n) => log::info!("Server max file size: {n} bytes"),
    }
    let value = resolve_server_max_file_size(advertised);
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
/// ### Errors
/// Returns a `SynchronizationError` if both the v2 and the legacy v1 begin
/// requests fail (network failure, authentication failure, or invalid response).
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
    let server_max_file_size = resolve_server_max_file_size(begin_v2.max_file_size_bytes);
    let shares: Vec<_> = std::thread::scope(|scope| {
        let handles: Vec<_> = begin_v2
            .share_ids
            .iter()
            .map(|id| {
                scope.spawn(move || {
                    (
                        id.as_str(),
                        share::fetch_share_by_id(
                            profile,
                            token_state,
                            http_agent,
                            id,
                            server_max_file_size,
                        ),
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
    Ok(begin_response)
}

/// Ping an authenticated Fulgurant server endpoint to test connectivity and credentials.
///
/// ### Arguments
/// - `server_url`: The base URL of the server (e.g. `https://example.com`).
/// - `token`: A valid JWT Bearer token.
/// - `http_agent`: Shared HTTP agent.
///
/// ### Errors
/// Returns a `SynchronizationError` if the server is unreachable, the token is
/// rejected, or the response cannot be parsed as the expected `ok: true` body.
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
    let sse_http_agent = Arc::clone(&shared.sse_http_agent);
    for profile in active_profiles {
        let sync_state = shared.sync_state_for(&profile.id);
        let (sse_tx, sse_shutdown_flag, sse_thread_handle) = {
            let mut sse = sync_state.sse.lock();
            let sse_tx = sse.sse_event_tx.clone();
            let shutdown_flag = Arc::new(AtomicBool::new(false));
            sse.sse_shutdown_flag = Some(Arc::clone(&shutdown_flag));
            let thread_handle = Arc::clone(&sse.sse_thread_handle);
            (sse_tx, Some(shutdown_flag), Some(thread_handle))
        };
        let http_agent_clone = Arc::clone(&http_agent);
        let sse_http_agent_clone = Arc::clone(&sse_http_agent);
        thread::spawn(move || {
            run_profile_bootstrap(
                &profile,
                &sync_state,
                sse_tx,
                sse_shutdown_flag,
                sse_thread_handle,
                &http_agent_clone,
                &sse_http_agent_clone,
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
/// - `http_agent`: Shared HTTP agent for short-lived REST calls.
/// - `sse_http_agent`: Dedicated long-timeout HTTP agent for the SSE stream.
fn run_profile_bootstrap(
    profile: &ServerProfile,
    sync_state: &Arc<SyncState>,
    sse_tx: Option<std::sync::mpsc::Sender<crate::fulgur::sync::sse::SseEvent>>,
    sse_shutdown_flag: Option<Arc<AtomicBool>>,
    sse_thread_handle: Option<Arc<Mutex<Option<thread::JoinHandle<()>>>>>,
    http_agent: &Arc<ureq::Agent>,
    sse_http_agent: &Arc<ureq::Agent>,
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
                *files = begin_response.shares;
            }
            if let (Some(tx), Some(shutdown), Some(handle_storage)) =
                (sse_tx, sse_shutdown_flag, sse_thread_handle)
            {
                log::info!(
                    "Profile '{}': starting SSE connection for real-time updates",
                    profile.name
                );
                let agents = SseAgents {
                    rest: Arc::clone(http_agent),
                    stream: Arc::clone(sse_http_agent),
                };
                let share_state = SseShareState {
                    pending_shared_files: Arc::clone(&sync_state.pending_shared_files),
                    max_file_size_bytes: Arc::clone(&sync_state.max_file_size_bytes),
                };
                match connect_sse(
                    profile,
                    tx,
                    shutdown,
                    sync_state.connection_status.clone(),
                    &sync_state.token_state,
                    &agents,
                    &share_state,
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
            SynchronizationError::InvalidResponse(e) | SynchronizationError::Other(e) => {
                write!(f, "{e}")
            }
            SynchronizationError::MissingEncryptionKey => write!(f, "Missing encryption key"),
            SynchronizationError::MissingExpirationDate => write!(f, "Missing expiration date"),
            SynchronizationError::MissingPublicKey(e) => {
                write!(f, "Missing public key for device: {e}")
            }
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
            SynchronizationError::HostNotFound
            | SynchronizationError::ConnectionFailed
            | SynchronizationError::Timeout(_) => SynchronizationStatus::ConnectionFailed,
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
            SynchronizationStatus::Connecting
            | SynchronizationStatus::Disconnected
            | SynchronizationStatus::AuthenticationFailed
            | SynchronizationStatus::ConnectionFailed
            | SynchronizationStatus::Other
            | SynchronizationStatus::NotActivated => false,
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
        let active_profiles: Vec<(crate::fulgur::settings::ProfileId, String)> = self
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .filter(|p| p.is_active)
            .map(|p| (p.id.clone(), p.name.clone()))
            .collect();
        for (profile_id, profile_name) in active_profiles {
            let sync_state = Fulgur::shared_state(cx).sync_state_for(&profile_id);
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
                *sync_state.last_share_receive_error_signature.lock() = None;
                continue;
            }
            let server_max_size = sync_state
                .max_file_size_bytes
                .load(std::sync::atomic::Ordering::Acquire);
            let mut shared_files_iter = shared_files_to_open.into_iter();
            let mut retry_queue = Vec::new();
            let mut key_unavailable = false;
            let mut key_load_failed = false;
            let mut decrypt_failures = 0usize;
            let mut opened_files = 0usize;

            while let Some(shared_file) = shared_files_iter.next() {
                if server_max_size != u64::MAX
                    && shared_file.content.len() as u64 > server_max_size.saturating_mul(2)
                {
                    log::warn!(
                        "Skipping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds the server max ({} bytes)",
                        shared_file.file_name,
                        shared_file.source_device_id,
                        shared_file.content.len(),
                        server_max_size
                    );
                    continue;
                }

                let encryption_key = match load_private_key_from_keychain(&profile_id) {
                    Ok(Some(key)) => key,
                    Ok(None) => {
                        key_unavailable = true;
                        log::warn!(
                            "Deferring {} shared file(s) for profile {profile_id}: encryption key is unavailable",
                            1 + shared_files_iter.len()
                        );
                        retry_queue.push(shared_file);
                        retry_queue.extend(shared_files_iter);
                        break;
                    }
                    Err(e) => {
                        key_load_failed = true;
                        log::warn!(
                            "Deferring {} shared file(s) for profile {profile_id}: failed to load encryption key from keychain: {e}",
                            1 + shared_files_iter.len()
                        );
                        retry_queue.push(shared_file);
                        retry_queue.extend(shared_files_iter);
                        break;
                    }
                };

                let decrypted_result =
                    crypto_helper::decrypt_bytes(&shared_file.content, encryption_key.as_str())
                        .and_then(|compressed_bytes| {
                            share::decompress_content(&compressed_bytes, server_max_size)
                        });
                drop(encryption_key);

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
                        opened_files += 1;
                        log::info!("Opened shared file: {}", shared_file.file_name);
                    }
                    Err(e) => {
                        decrypt_failures += 1;
                        log::warn!(
                            "Deferring shared file '{}' for profile {profile_id}: decryption failed ({e})",
                            shared_file.file_name
                        );
                        retry_queue.push(shared_file);
                    }
                }
            }

            let mut retry_count = 0usize;
            if !retry_queue.is_empty() {
                retry_count = retry_queue.len();
                let mut pending = sync_state.pending_shared_files.lock();
                retry_queue.extend(std::mem::take(&mut *pending));
                *pending = retry_queue;
                log::warn!(
                    "Re-queued {retry_count} shared file(s) for profile {profile_id} for retry"
                );
            }

            let error_notification = if key_unavailable {
                Some((
                    "missing-keychain-private-key",
                    SharedString::from(format!(
                        "{profile_name}: Cannot receive shared files because the encryption key is unavailable in the keychain. Fulgur will retry automatically."
                    )),
                ))
            } else if key_load_failed {
                Some((
                    "failed-to-load-keychain-private-key",
                    SharedString::from(format!(
                        "{profile_name}: Cannot receive shared files because the encryption key could not be loaded from the keychain. Fulgur will retry automatically."
                    )),
                ))
            } else if decrypt_failures > 0 {
                Some((
                    "share-decryption-failed",
                    SharedString::from(format!(
                        "{profile_name}: Failed to decrypt {decrypt_failures} shared file(s). Fulgur will retry automatically."
                    )),
                ))
            } else {
                None
            };

            if let Some((signature, message)) = error_notification {
                let mut last_signature = sync_state.last_share_receive_error_signature.lock();
                if last_signature.as_deref() != Some(signature) {
                    *sync_state.pending_notification.lock() =
                        Some((NotificationType::Error, message));
                    *last_signature = Some(signature.to_string());
                }
            } else if opened_files > 0 || retry_count == 0 {
                *sync_state.last_share_receive_error_signature.lock() = None;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES, MAX_HTTP_V1_BEGIN_RESPONSE_BYTES,
        max_http_bulk_shares_response_bytes, max_http_single_share_response_bytes,
        resolve_server_max_file_size, store_server_max_file_size,
    };
    use crate::fulgur::sync::share;
    use std::sync::atomic::{AtomicU64, Ordering};

    #[test]
    fn bulk_cap_falls_back_to_static_bound_when_unlimited() {
        assert_eq!(
            max_http_bulk_shares_response_bytes(u64::MAX),
            MAX_HTTP_V1_BEGIN_RESPONSE_BYTES
        );
    }

    #[test]
    fn bulk_cap_derives_from_server_limit() {
        let server_max = 2 * 1024 * 1024;
        let per_share = server_max * 2 + share::JSON_OVERHEAD_PER_SHARE_BYTES as u64;
        let expected = (share::MAX_PENDING_SHARES_PER_RESPONSE as u64) * per_share + 4 * 1024;
        assert_eq!(max_http_bulk_shares_response_bytes(server_max), expected);
    }

    #[test]
    fn bulk_cap_saturates_instead_of_overflowing() {
        assert_eq!(max_http_bulk_shares_response_bytes(u64::MAX - 1), u64::MAX);
    }

    #[test]
    fn single_share_cap_falls_back_to_static_bound_when_unlimited() {
        assert_eq!(
            max_http_single_share_response_bytes(u64::MAX),
            MAX_HTTP_SINGLE_SHARE_RESPONSE_BYTES
        );
    }

    #[test]
    fn single_share_cap_derives_from_server_limit() {
        let server_max = 5 * 1024 * 1024;
        let expected = server_max * 2 + share::JSON_OVERHEAD_PER_SHARE_BYTES as u64 + 4 * 1024;
        assert_eq!(max_http_single_share_response_bytes(server_max), expected);
    }

    #[test]
    fn single_share_cap_covers_incompressible_default_file() {
        let default_plaintext = share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64;
        let worst_case_wire = default_plaintext * 137 / 100;
        assert!(max_http_single_share_response_bytes(u64::MAX) >= worst_case_wire);
        assert!(max_http_single_share_response_bytes(default_plaintext) >= worst_case_wire);
    }

    #[test]
    fn single_share_cap_saturates_instead_of_overflowing() {
        assert_eq!(max_http_single_share_response_bytes(u64::MAX - 1), u64::MAX);
    }

    #[test]
    fn resolve_maps_none_to_unlimited() {
        assert_eq!(resolve_server_max_file_size(None), u64::MAX);
    }

    #[test]
    fn resolve_maps_zero_to_safe_default() {
        assert_eq!(
            resolve_server_max_file_size(Some(0)),
            share::MAX_SYNC_SHARE_PAYLOAD_BYTES as u64
        );
    }

    #[test]
    fn resolve_trusts_positive_values() {
        assert_eq!(resolve_server_max_file_size(Some(7 * 1024)), 7 * 1024);
    }

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
