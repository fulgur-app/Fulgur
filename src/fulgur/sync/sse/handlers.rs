use crate::fulgur::{
    Fulgur,
    sync::synchronization::{SynchronizationStatus, initial_synchronization},
    utils::utilities::collect_events,
};
use gpui::{App, Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::{connection::connect_sse, types::SseEvent};

/// Maximum time to wait for the previous SSE thread to exit before starting a new one
const SSE_THREAD_SHUTDOWN_TIMEOUT: Duration = Duration::from_secs(5);

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
            let pending_shared_files =
                Arc::clone(&self.shared_state(cx).sync_state.pending_shared_files);
            let handle_storage = Arc::clone(&self.sse_state.sse_thread_handle);
            thread::spawn(move || {
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
                thread::sleep(Duration::from_millis(200));
                match initial_synchronization(
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
                            pending_shared_files,
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
                log::debug!(
                    "Share doorbell on UI tick (share_id={})",
                    notification.share_id
                );
                let message = SharedString::from("New file received".to_string());
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
    use super::super::{ShareNotification, SseEvent, SseState};
    use crate::fulgur::{
        Fulgur, settings::Settings, shared_state::SharedAppState,
        sync::synchronization::SynchronizationStatus, window_manager::WindowManager,
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
                this.handle_sse_event(
                    SseEvent::Heartbeat {
                        timestamp: "ts1".to_string(),
                    },
                    window,
                    cx,
                );
                *this.shared_state(cx).sync_state.connection_status.lock() =
                    SynchronizationStatus::Disconnected;
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
    fn test_handle_share_available_does_not_touch_pending_files(cx: &mut TestAppContext) {
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
                let notification = make_share_notification("share-abc");
                this.handle_sse_event(SseEvent::ShareAvailable(notification), window, cx);
                assert!(
                    this.shared_state(cx)
                        .sync_state
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
                drop(tx);
                this.process_sse_events(window, cx);
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
