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
                Fulgur::shared_state(cx)
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
                Fulgur::shared_state(cx)
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
            *Fulgur::shared_state(cx)
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
                Fulgur::shared_state(cx)
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
            *Fulgur::shared_state(cx)
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
                Fulgur::shared_state(cx)
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
            *Fulgur::shared_state(cx)
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
                !Fulgur::shared_state(cx)
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
                Fulgur::shared_state(cx)
                    .primary_sync_state()
                    .pending_shared_files
                    .lock()
                    .is_empty(),
                "pending_shared_files should start empty"
            );
            let notification = make_share_notification("share-abc");
            this.handle_sse_event(SseEvent::ShareAvailable(notification), window, cx);
            assert!(
                Fulgur::shared_state(cx)
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
                Fulgur::shared_state(cx)
                    .primary_sync_state()
                    .last_heartbeat
                    .lock()
                    .is_none()
            );
            assert!(
                Fulgur::shared_state(cx)
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
                Fulgur::shared_state(cx)
                    .primary_sync_state()
                    .last_heartbeat
                    .lock()
                    .is_none()
            );
            assert!(
                Fulgur::shared_state(cx)
                    .primary_sync_state()
                    .pending_shared_files
                    .lock()
                    .is_empty()
            );
        });
    });
}

// --- process_sse_events ---

/// Install a fresh SSE channel on the shared sync state for the empty profile
/// id used by the Phase 1 single-profile tests. Returns the `Sender` for the
/// test to emit events through.
fn install_test_sse_channel(cx: &gpui::App) -> std::sync::mpsc::Sender<SseEvent> {
    let (tx, rx) = std::sync::mpsc::channel();
    let sync_state = Fulgur::shared_state(cx).sync_state_for("");
    let mut sse = sync_state.sse.lock();
    sse.sse_event_tx = Some(tx.clone());
    sse.sse_events = Some(rx);
    tx
}

#[gpui::test]
fn test_process_sse_events_dispatches_heartbeat_from_channel(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|window, cx| {
        fulgur.update(cx, |this, cx| {
            let tx = install_test_sse_channel(cx);
            tx.send(SseEvent::Heartbeat {
                timestamp: "ts".to_string(),
            })
            .unwrap();
            assert!(
                Fulgur::shared_state(cx)
                    .primary_sync_state()
                    .last_heartbeat
                    .lock()
                    .is_none()
            );
            this.process_sse_events(window, cx);
            assert!(
                Fulgur::shared_state(cx)
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
            let _tx = install_test_sse_channel(cx);
            this.process_sse_events(window, cx);
            assert!(
                Fulgur::shared_state(cx)
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
            let tx = install_test_sse_channel(cx);
            drop(tx);
            this.process_sse_events(window, cx);
            assert!(
                Fulgur::shared_state(cx)
                    .primary_sync_state()
                    .last_heartbeat
                    .lock()
                    .is_none(),
                "No events dispatched from closed channel"
            );
        });
    });
}
