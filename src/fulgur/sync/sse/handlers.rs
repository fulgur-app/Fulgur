use crate::fulgur::ui::notifications::progress::{CancelCallback, start_progress};
use crate::fulgur::{
    Fulgur,
    settings::{ProfileId, ServerProfile},
    sync::synchronization::{
        SynchronizationStatus, initial_synchronization, set_sync_server_connection_status,
        store_server_max_file_size,
    },
    utils::utilities::collect_events,
};
use gpui::{App, Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
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

use super::{connection::connect_sse, types::SseEvent, types::SseState};

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
    /// Check if any profile is currently connected to its sync server.
    ///
    /// ### Returns
    /// - `true`: At least one profile reports a connected status.
    /// - `false`: No profile is connected.
    pub fn is_connected(&self, cx: &App) -> bool {
        let shared = self.shared_state(cx);
        let states = shared.sync_states.read();
        states
            .values()
            .any(|s| s.connection_status.lock().is_connected())
    }

    /// Signal the old SSE worker's shutdown flag, allocate a fresh SSE state
    /// slot, and validate that the profile is ready to connect.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile to restart.
    ///
    /// ### Returns
    /// - `Some(SseRestartSetup)`: Setup resources ready for the caller to spawn a thread.
    /// - `None`: The profile was not found, is inactive, or the master switch is off.
    fn prepare_sse_restart(&mut self, profile_id: &str) -> Option<SseRestartSetup> {
        if let Some(state) = self.sse_states.get(profile_id)
            && let Some(ref shutdown_flag) = state.sse_shutdown_flag
        {
            log::info!("Profile '{profile_id}': signaling SSE shutdown");
            shutdown_flag.store(true, Ordering::Relaxed);
        }
        let old_handle = self
            .sse_states
            .get(profile_id)
            .map(|s| s.sse_thread_handle.lock().take())
            .unwrap_or(None);
        let (sse_tx, sse_rx) = std::sync::mpsc::channel();
        let sse_shutdown_flag = Arc::new(AtomicBool::new(false));
        let mut new_state = SseState::new();
        new_state.sse_events = Some(sse_rx);
        new_state.sse_event_tx = Some(sse_tx.clone());
        new_state.sse_shutdown_flag = Some(sse_shutdown_flag.clone());
        let handle_storage = Arc::clone(&new_state.sse_thread_handle);
        self.sse_states.insert(profile_id.to_string(), new_state);

        let profile = match self
            .settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .find(|p| p.id == profile_id)
        {
            Some(p) => p.clone(),
            None => {
                log::warn!("prepare_sse_restart: profile id '{profile_id}' not found in settings");
                return None;
            }
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
        }) = self.prepare_sse_restart(profile_id)
        else {
            return;
        };
        let shared = self.shared_state(cx);
        let sync_state = shared.sync_state_for(&profile.id);
        let sync_status = sync_state.connection_status.clone();
        let token_state = Arc::clone(&sync_state.token_state);
        let http_agent = Arc::clone(&shared.http_agent);
        let pending_shared_files = Arc::clone(&sync_state.pending_shared_files);
        thread::spawn(move || {
            wait_for_previous_sse_thread(old_handle);
            thread::sleep(Duration::from_millis(200));
            match initial_synchronization(&profile, &token_state, &http_agent) {
                Ok(_) => {
                    log::info!(
                        "Profile '{}': initial sync succeeded, starting new SSE",
                        profile.name
                    );
                    match connect_sse(
                        &profile,
                        sse_tx,
                        sse_shutdown_flag,
                        sync_status,
                        &token_state,
                        &http_agent,
                        &pending_shared_files,
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
        }) = self.prepare_sse_restart(profile_id)
        else {
            return;
        };
        let shared = self.shared_state(cx);
        let sync_state = shared.sync_state_for(&profile.id);
        let connection_status = sync_state.connection_status.clone();
        let connecting_since = sync_state.connecting_since.clone();
        let token_state = Arc::clone(&sync_state.token_state);
        let http_agent = Arc::clone(&shared.http_agent);
        let pending_shared_files = Arc::clone(&sync_state.pending_shared_files);
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
                        match connect_sse(
                            &profile,
                            sse_tx,
                            sse_shutdown_flag,
                            connection_status.clone(),
                            &token_state,
                            &http_agent,
                            &pending_shared_files,
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

    /// Handle a single SSE event for a specific profile.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile that produced the event.
    /// - `event`: The SSE event to handle.
    /// - `window`: The window to show notifications in.
    /// - `cx`: The application context.
    pub fn handle_sse_event_for(
        &mut self,
        profile_id: &ProfileId,
        event: SseEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let state = self.sse_states.entry(profile_id.clone()).or_default();
        if let Some(last_time) = state.last_sse_event
            && now.duration_since(last_time) < Duration::from_millis(500)
        {
            return;
        }
        state.last_sse_event = Some(now);
        let sync_state = self.shared_state(cx).sync_state_for(profile_id);
        match event {
            SseEvent::Heartbeat { timestamp } => {
                log::debug!("SSE heartbeat received for profile '{profile_id}': {timestamp}");
                let was_disconnected = !sync_state.connection_status.lock().is_connected();
                *sync_state.last_heartbeat.lock() = Some(now);
                if was_disconnected {
                    *sync_state.connection_status.lock() = SynchronizationStatus::Connected;
                    log::info!(
                        "Profile '{profile_id}': connection restored on heartbeat after timeout"
                    );
                }
            }
            SseEvent::ShareAvailable(notification) => {
                log::debug!(
                    "Share doorbell on UI tick (share_id={})",
                    notification.share_id
                );
                let message = SharedString::from("New file received".to_string());
                window.push_notification((NotificationType::Info, message), cx);
            }
            SseEvent::Error(err) => {
                log::error!("SSE error for profile '{profile_id}': {err}");
            }
        }
    }

    /// Handle an SSE event using the primary profile.
    ///
    /// ### Arguments
    /// - `event`: The SSE event to handle.
    /// - `window`: The window to show notifications in.
    /// - `cx`: The application context.
    pub fn handle_sse_event(
        &mut self,
        event: SseEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let primary_id = self
            .settings
            .app_settings
            .synchronization_settings
            .primary_profile()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.handle_sse_event_for(&primary_id, event, window, cx);
    }

    /// Drain pending SSE events from every profile's channel and dispatch them.
    ///
    /// ### Arguments
    /// - `window`: The window to handle events in.
    /// - `cx`: The application context.
    pub fn process_sse_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile_ids: Vec<ProfileId> = self.sse_states.keys().cloned().collect();
        for profile_id in profile_ids {
            let events = self
                .sse_states
                .get(&profile_id)
                .map(|s| collect_events(&s.sse_events))
                .unwrap_or_default();
            for event in events {
                self.handle_sse_event_for(&profile_id, event, window, cx);
            }
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::super::{ShareNotification, SseEvent, SseState};
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState,
        sync::synchronization::SynchronizationStatus, window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
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
                cx.open_window(WindowOptions::default(), |window, cx| {
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
    fn make_share_notification(share_id: &str) -> ShareNotification {
        ShareNotification {
            share_id: share_id.to_string(),
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
                        .primary_sync_state()
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
                        .primary_sync_state()
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
                *this
                    .shared_state(cx)
                    .primary_sync_state()
                    .connection_status
                    .lock() = SynchronizationStatus::Disconnected;
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts".to_string(),
                    },
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
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
                *this
                    .shared_state(cx)
                    .primary_sync_state()
                    .connection_status
                    .lock() = SynchronizationStatus::Connected;
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts".to_string(),
                    },
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
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
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts1".to_string(),
                    },
                    window,
                    cx,
                );
                *this
                    .shared_state(cx)
                    .primary_sync_state()
                    .connection_status
                    .lock() = SynchronizationStatus::Disconnected;
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts2".to_string(),
                    },
                    window,
                    cx,
                );
                assert!(
                    !this
                        .shared_state(cx)
                        .primary_sync_state()
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
    fn test_handle_share_available_does_not_touch_pending_files(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .pending_shared_files
                        .lock()
                        .is_empty(),
                    "pending_shared_files should start empty"
                );
                let notification = make_share_notification("share-abc");
                this.handle_sse_event(SseEvent::ShareAvailable(notification), window, cx);
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .pending_shared_files
                        .lock()
                        .is_empty(),
                    "UI doorbell handler must not push into pending_shared_files; \
                     the SSE worker drains via /api/shares instead"
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
                        .primary_sync_state()
                        .last_heartbeat
                        .lock()
                        .is_none()
                );
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .pending_shared_files
                        .lock()
                        .is_empty()
                );
                this.handle_sse_event(
                    SseEvent::Error("connection timeout".to_string()),
                    window,
                    cx,
                );
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .last_heartbeat
                        .lock()
                        .is_none()
                );
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .pending_shared_files
                        .lock()
                        .is_empty()
                );
            });
        });
    }

    // --- process_sse_events ---

    /// Insert (or replace) the SSE channel for the empty profile id used by
    /// the Phase 1 single-profile tests. Returns the `Sender` for the test to
    /// emit events through.
    fn install_test_sse_channel(this: &mut Fulgur) -> std::sync::mpsc::Sender<SseEvent> {
        let (tx, rx) = std::sync::mpsc::channel();
        let mut state = SseState::new();
        state.sse_events = Some(rx);
        this.sse_states.insert(String::new(), state);
        tx
    }

    #[gpui::test]
    fn test_process_sse_events_dispatches_heartbeat_from_channel(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let tx = install_test_sse_channel(this);
                tx.send(SseEvent::Heartbeat {
                    timestamp: "ts".to_string(),
                })
                .unwrap();
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .last_heartbeat
                        .lock()
                        .is_none()
                );
                this.process_sse_events(window, cx);
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
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
                let _tx = install_test_sse_channel(this);
                this.process_sse_events(window, cx);
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
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
                let tx = install_test_sse_channel(this);
                drop(tx);
                this.process_sse_events(window, cx);
                assert!(
                    this.shared_state(cx)
                        .primary_sync_state()
                        .last_heartbeat
                        .lock()
                        .is_none(),
                    "No events dispatched from closed channel"
                );
            });
        });
    }
}
