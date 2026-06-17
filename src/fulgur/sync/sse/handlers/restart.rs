use crate::fulgur::ui::notifications::progress::{CancelCallback, start_progress};
use crate::fulgur::{
    Fulgur,
    settings::ServerProfile,
    sync::synchronization::{
        SynchronizationStatus, initial_synchronization, set_sync_server_connection_status,
        store_server_max_file_size,
    },
};
use gpui::{Context, SharedString, Window};
use gpui_component::notification::NotificationType;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc::Sender,
    },
    thread,
    time::{Duration, Instant},
};

use super::super::{
    connection::{SseAgents, SseShareState, connect_sse},
    types::SseEvent,
};

/// Maximum time to wait for the previous SSE thread to exit before starting a new one
const SSE_THREAD_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

/// Resources produced by the common SSE restart setup phase.
struct SseRestartSetup {
    old_handle: Option<thread::JoinHandle<()>>,
    sse_tx: Sender<SseEvent>,
    sse_shutdown_flag: Arc<AtomicBool>,
    handle_storage: Arc<Mutex<Option<thread::JoinHandle<()>>>>,
    profile: ServerProfile,
}

/// Wait for a previous SSE background thread to stop before proceeding.
///
/// ### Arguments
/// - `old_handle`: The join handle from the previous SSE thread, if any.
fn wait_for_previous_sse_thread(old_handle: Option<thread::JoinHandle<()>>) {
    let Some(handle) = old_handle else { return };
    let deadline = Instant::now() + SSE_THREAD_SHUTDOWN_TIMEOUT;
    while !handle.is_finished() && Instant::now() < deadline {
        thread::sleep(Duration::from_millis(100));
    }
    if handle.is_finished() {
        let _ = handle.join();
        log::info!("Previous SSE thread exited");
    } else {
        log::warn!(
            "Previous SSE thread still running after {SSE_THREAD_SHUTDOWN_TIMEOUT:?}, proceeding with new connection"
        );
    }
}

impl Fulgur {
    /// Signal the old SSE worker's shutdown flag, rotate in a fresh shutdown
    /// flag on the shared SSE state, and validate that the profile is ready to
    /// connect.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile to restart.
    /// - `cx`: The Fulgur context.
    ///
    /// ### Returns
    /// - `Some(SseRestartSetup)`: Setup resources ready for the caller to spawn a thread.
    /// - `None`: The profile was not found, is inactive, or the master switch is off.
    fn prepare_sse_restart(
        &mut self,
        profile_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<SseRestartSetup> {
        let sync_state = Fulgur::shared_state(cx).sync_state_for(profile_id);
        let (sse_tx, sse_shutdown_flag, handle_storage, old_handle) = {
            let mut sse = sync_state.sse.lock();
            if let Some(ref shutdown_flag) = sse.sse_shutdown_flag {
                log::info!("Profile '{profile_id}': signaling SSE shutdown");
                shutdown_flag.store(true, Ordering::Relaxed);
            }
            let old_handle = sse.sse_thread_handle.lock().take();
            let sse_tx = sse
                .sse_event_tx
                .clone()
                .expect("shared SSE state must own a live event sender");
            let sse_shutdown_flag = Arc::new(AtomicBool::new(false));
            sse.sse_shutdown_flag = Some(Arc::clone(&sse_shutdown_flag));
            let handle_storage = Arc::clone(&sse.sse_thread_handle);
            (sse_tx, sse_shutdown_flag, handle_storage, old_handle)
        };

        let profile = if let Some(p) = self
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
        {
            p.clone()
        } else {
            log::warn!("prepare_sse_restart: profile id '{profile_id}' not found in settings");
            return None;
        };
        let master_on = self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated;
        if !master_on || !profile.is_active {
            log::info!(
                "Profile '{}' not active or master switch off, SSE connection not started",
                profile.name
            );
            return None;
        }
        Some(SseRestartSetup {
            old_handle,
            sse_tx,
            sse_shutdown_flag,
            handle_storage,
            profile,
        })
    }

    /// Restart the SSE connection for a single profile.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose SSE worker should be restarted.
    /// - `cx`: The context of the application.
    pub fn restart_sse_connection_for(&mut self, profile_id: &str, cx: &mut Context<Self>) {
        let Some(SseRestartSetup {
            old_handle,
            sse_tx,
            sse_shutdown_flag,
            handle_storage,
            profile,
        }) = self.prepare_sse_restart(profile_id, cx)
        else {
            return;
        };
        let shared = Fulgur::shared_state(cx);
        let sync_state = shared.sync_state_for(&profile.id);
        let sync_status = sync_state.connection_status.clone();
        let token_state = Arc::clone(&sync_state.token_state);
        let http_agent = Arc::clone(&shared.http_agent);
        let sse_http_agent = Arc::clone(&shared.sse_http_agent);
        let pending_shared_files = Arc::clone(&sync_state.pending_shared_files);
        let pending_ack_share_ids = Arc::clone(&sync_state.pending_ack_share_ids);
        let max_file_size_bytes = Arc::clone(&sync_state.max_file_size_bytes);
        thread::spawn(move || {
            wait_for_previous_sse_thread(old_handle);
            thread::sleep(Duration::from_millis(200));
            match initial_synchronization(&profile, &token_state, &http_agent) {
                Ok(_) => {
                    log::info!(
                        "Profile '{}': initial sync succeeded, starting new SSE",
                        profile.name
                    );
                    let agents = SseAgents {
                        rest: Arc::clone(&http_agent),
                        stream: Arc::clone(&sse_http_agent),
                    };
                    let share_state = SseShareState {
                        pending_shared_files: Arc::clone(&pending_shared_files),
                        pending_ack_share_ids: Arc::clone(&pending_ack_share_ids),
                        max_file_size_bytes: Arc::clone(&max_file_size_bytes),
                    };
                    match connect_sse(
                        &profile,
                        sse_tx,
                        sse_shutdown_flag,
                        sync_status,
                        &token_state,
                        &agents,
                        &share_state,
                    ) {
                        Ok(new_handle) => {
                            *handle_storage.lock() = Some(new_handle);
                        }
                        Err(e) => {
                            log::error!("Profile '{}': failed to start SSE: {e}", profile.name);
                        }
                    }
                }
                Err(e) => {
                    log::error!(
                        "Profile '{}': initial sync failed, not starting SSE: {e}",
                        profile.name
                    );
                }
            }
        });
    }

    /// Restart the SSE connection for a single profile, showing a progress
    /// indicator and a success/error notification when the connection attempt
    /// completes.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose SSE worker should be restarted.
    /// - `window`: The window to attach the progress indicator to.
    /// - `cx`: The context of the application.
    pub fn restart_sse_connection_for_with_progress(
        &mut self,
        profile_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(SseRestartSetup {
            old_handle,
            sse_tx,
            sse_shutdown_flag,
            handle_storage,
            profile,
        }) = self.prepare_sse_restart(profile_id, cx)
        else {
            return;
        };
        let shared = Fulgur::shared_state(cx);
        let sync_state = shared.sync_state_for(&profile.id);
        let connection_status = sync_state.connection_status.clone();
        let connecting_since = sync_state.connecting_since.clone();
        let token_state = Arc::clone(&sync_state.token_state);
        let http_agent = Arc::clone(&shared.http_agent);
        let sse_http_agent = Arc::clone(&shared.sse_http_agent);
        let pending_shared_files = Arc::clone(&sync_state.pending_shared_files);
        let pending_ack_share_ids = Arc::clone(&sync_state.pending_ack_share_ids);
        let pending_notification = Arc::clone(&sync_state.pending_notification);
        let device_name = sync_state.device_name.clone();
        let max_file_size_bytes = Arc::clone(&sync_state.max_file_size_bytes);
        let profile_name = profile.name.clone();

        set_sync_server_connection_status(&connection_status, SynchronizationStatus::Connecting);
        *connecting_since.lock() = Some(Instant::now());

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
            wait_for_previous_sse_thread(old_handle);
            thread::sleep(Duration::from_millis(200));

            if cancel_flag_for_thread.load(Ordering::Acquire) {
                done_for_thread.store(true, Ordering::Release);
                return;
            }

            let (notification, status) =
                match initial_synchronization(&profile, &token_state, &http_agent) {
                    Ok(begin_response) => {
                        store_server_max_file_size(
                            &max_file_size_bytes,
                            begin_response.max_file_size_bytes,
                        );
                        *device_name.lock() = Some(begin_response.device_name.clone());
                        *pending_shared_files.lock() = begin_response.shares;
                        let msg = format!(
                            "{profile_name}: Connection successful as {}",
                            begin_response.device_name
                        );
                        let agents = SseAgents {
                            rest: Arc::clone(&http_agent),
                            stream: Arc::clone(&sse_http_agent),
                        };
                        let share_state = SseShareState {
                            pending_shared_files: Arc::clone(&pending_shared_files),
                            pending_ack_share_ids: Arc::clone(&pending_ack_share_ids),
                            max_file_size_bytes: Arc::clone(&max_file_size_bytes),
                        };
                        match connect_sse(
                            &profile,
                            sse_tx,
                            sse_shutdown_flag,
                            connection_status.clone(),
                            &token_state,
                            &agents,
                            &share_state,
                        ) {
                            Ok(new_handle) => {
                                *handle_storage.lock() = Some(new_handle);
                            }
                            Err(e) => {
                                log::error!("Profile '{}': failed to start SSE: {e}", profile.name);
                            }
                        }
                        (
                            (NotificationType::Success, SharedString::from(msg)),
                            SynchronizationStatus::Connected,
                        )
                    }
                    Err(e) => {
                        let msg = format!("{profile_name}: Connection failed: {e}");
                        (
                            (NotificationType::Error, SharedString::from(msg)),
                            SynchronizationStatus::from_error(&e),
                        )
                    }
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
}
