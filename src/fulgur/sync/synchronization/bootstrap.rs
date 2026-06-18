use super::begin::initial_synchronization;
use super::error::SynchronizationStatus;
use super::limits::store_server_max_file_size;
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::shared_state::SyncState;
use crate::fulgur::sync::sse::{SseAgents, SseShareState, connect_sse};
use crate::fulgur::ui::notifications::progress::{CancelCallback, start_progress};
use crate::fulgur::utils::crypto_helper::load_device_api_key_from_keychain;
use gpui::{App, SharedString, Window};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::thread;
use std::time::{Duration, Instant};

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
    match initial_synchronization(
        profile,
        &sync_state.token_state,
        http_agent,
        &sync_state.pending_ack_share_ids,
    ) {
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
                    pending_ack_share_ids: Arc::clone(&sync_state.pending_ack_share_ids),
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

/// Perform initial synchronization with a single profile's server in a background thread.
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
    let pending_ack_share_ids = sync_state.pending_ack_share_ids.clone();
    let pending_notification = sync_state.pending_notification.clone();
    let max_file_size_bytes = sync_state.max_file_size_bytes.clone();
    thread::spawn(move || {
        let result =
            initial_synchronization(&profile, &token_state, &http_agent, &pending_ack_share_ids);
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

/// Perform initial synchronization with a single profile's server, showing a progress spinner.
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
    let pending_ack_share_ids = sync_state.pending_ack_share_ids.clone();
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
        let result =
            initial_synchronization(&profile, &token_state, &http_agent, &pending_ack_share_ids);

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
