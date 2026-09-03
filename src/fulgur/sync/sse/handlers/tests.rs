use super::super::{ShareNotification, SseEvent, SseState, types::SSE_WORKER_JOIN_TIMEOUT};
use crate::fulgur::{
    Fulgur,
    settings::{ServerProfile, Settings},
    shared_state::SharedAppState,
    sync::synchronization::SynchronizationStatus,
    utils::worker::Worker,
    window_manager::WindowManager,
};
use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
use parking_lot::Mutex;
use std::{
    cell::RefCell,
    path::PathBuf,
    sync::Arc,
    thread,
    time::{Duration, Instant},
};

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
        cx.set_global(SharedAppState::new(settings, pending_files, None, None));
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

/// Install the globals event routing needs, without opening a window.
///
/// ### Arguments
/// - `cx`: The test application context to install the globals into.
fn setup_globals(cx: &mut TestAppContext) {
    cx.update(|cx| {
        let mut settings = Settings::new();
        settings.editor_settings.watch_files = false;
        let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
        cx.set_global(SharedAppState::new(settings, pending_files, None, None));
        cx.set_global(WindowManager::new());
    });
}

/// Build a minimal valid `ShareNotification` for use in tests.
fn make_share_notification(share_id: &str) -> ShareNotification {
    ShareNotification {
        share_id: share_id.to_string(),
    }
}

fn test_profile_id() -> String {
    String::new()
}

// --- SseState construction (no GPUI context needed) ---

#[test]
fn test_sse_state_new_is_fully_empty() {
    let state = SseState::new();
    assert!(state.sse_events.is_none());
    assert!(state.sse_event_tx.is_none());
    assert!(state.last_sse_event.is_none());
    assert!(state.worker.is_none());
}

// --- handle_sse_event_for_profile: Heartbeat ---

#[gpui::test]
fn test_handle_heartbeat_sets_last_heartbeat(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .last_heartbeat
                .lock()
                .is_none(),
            "last_heartbeat should start as None"
        );
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::Heartbeat {
                timestamp: "2024-01-01T00:00:00Z".to_string(),
            },
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
}

#[gpui::test]
fn test_handle_heartbeat_when_disconnected_restores_connected_status(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        *Fulgur::shared_state(cx)
            .primary_sync_state()
            .connection_status
            .lock() = SynchronizationStatus::Disconnected;
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::Heartbeat {
                timestamp: "ts".to_string(),
            },
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
}

#[gpui::test]
fn test_handle_heartbeat_when_connected_keeps_connected_status(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        *Fulgur::shared_state(cx)
            .primary_sync_state()
            .connection_status
            .lock() = SynchronizationStatus::Connected;
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::Heartbeat {
                timestamp: "ts".to_string(),
            },
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
}

// --- handle_sse_event_for_profile: debounce ---

/// Read the instant the debounce window was last opened for the test profile.
fn debounce_window_opened_at(cx: &gpui::App) -> Option<Instant> {
    Fulgur::shared_state(cx)
        .sync_state_for(&test_profile_id())
        .sse
        .lock()
        .last_sse_event
}

#[gpui::test]
fn test_share_doorbell_debounce_collapses_a_rapid_second_doorbell(cx: &mut TestAppContext) {
    // Collapsing doorbell storms is what the debounce exists for, and the only
    // event it may apply to.
    setup_globals(cx);
    cx.update(|cx| {
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::ShareAvailable(make_share_notification("share-1")),
            cx,
        );
        let window_opened_at = debounce_window_opened_at(cx);
        assert!(
            window_opened_at.is_some(),
            "a share doorbell must open the debounce window"
        );

        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::ShareAvailable(make_share_notification("share-2")),
            cx,
        );
        assert_eq!(
            debounce_window_opened_at(cx),
            window_opened_at,
            "a second doorbell inside the window must be collapsed into the first"
        );
    });
}

#[gpui::test]
fn test_a_heartbeat_is_never_swallowed_by_the_debounce(cx: &mut TestAppContext) {
    // A heartbeat is the profile's liveness signal, so honouring it must not
    // depend on how recently an unrelated event happened to arrive.
    setup_globals(cx);
    cx.update(|cx| {
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::ShareAvailable(make_share_notification("share-1")),
            cx,
        );
        *Fulgur::shared_state(cx)
            .primary_sync_state()
            .connection_status
            .lock() = SynchronizationStatus::Disconnected;

        // Immediately after the doorbell, well inside the debounce window.
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::Heartbeat {
                timestamp: "ts".to_string(),
            },
            cx,
        );
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .connection_status
                .lock()
                .is_connected(),
            "a heartbeat inside the debounce window must still restore Connected"
        );
    });
}

#[gpui::test]
fn test_pending_shares_snapshot_does_not_consume_the_debounce_window(cx: &mut TestAppContext) {
    // The server sends the pending-shares snapshot and a heartbeat in the same
    // burst after every reconnect. Sharing one window between them meant one of
    // the two was always silently discarded.
    setup_globals(cx);
    cx.update(|cx| {
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::PendingSharesSnapshot,
            cx,
        );
        assert_eq!(
            debounce_window_opened_at(cx),
            None,
            "the snapshot must not open a debounce window the heartbeat then falls into"
        );
    });
}

// --- handle_sse_event_for_profile: ShareAvailable ---

#[gpui::test]
fn test_handle_share_available_does_not_touch_pending_files(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .pending_shared_files
                .lock()
                .is_empty(),
            "pending_shared_files should start empty"
        );
        let notification = make_share_notification("share-abc");
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::ShareAvailable(notification),
            cx,
        );
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .pending_shared_files
                .lock()
                .is_empty(),
            "UI doorbell handler must not push into pending_shared_files; \
             the SSE worker fetches via /api/v2/shares/:id instead"
        );
    });
}

// --- handle_sse_event_for_profile: Error ---

#[gpui::test]
fn test_handle_error_event_does_not_change_shared_state(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
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
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::Error("connection timeout".to_string()),
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
}

// --- spawn_sse_event_consumer ---

/// Install a fresh SSE channel on the shared sync state for the empty profile
/// id used by the Phase 1 single-profile tests. Returns the `Sender` for the
/// test to emit events through.
fn install_test_sse_channel(cx: &gpui::App) -> futures::channel::mpsc::UnboundedSender<SseEvent> {
    let (tx, rx) = futures::channel::mpsc::unbounded();
    let sync_state = Fulgur::shared_state(cx).sync_state_for("");
    let mut sse = sync_state.sse.lock();
    sse.sse_event_tx = Some(tx.clone());
    sse.sse_events = Some(rx);
    tx
}

#[gpui::test]
fn test_sse_consumer_dispatches_heartbeat_from_channel(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        let tx = install_test_sse_channel(cx);
        Fulgur::spawn_sse_event_consumer("", cx);
        tx.unbounded_send(SseEvent::Heartbeat {
            timestamp: "ts".to_string(),
        })
        .unwrap();
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .last_heartbeat
                .lock()
                .is_none(),
            "the consumer task has not run yet, so the heartbeat must not be applied"
        );
    });
    visual_cx.run_until_parked();
    visual_cx.update(|_window, cx| {
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .last_heartbeat
                .lock()
                .is_some(),
            "Heartbeat from channel must be dispatched by the consumer task"
        );
    });
}

#[gpui::test]
fn test_sse_consumer_spawn_is_idempotent(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        let _tx = install_test_sse_channel(cx);
        Fulgur::spawn_sse_event_consumer("", cx);
        // The receiver was taken by the first consumer; a second spawn is a no-op.
        Fulgur::spawn_sse_event_consumer("", cx);
        assert!(
            Fulgur::shared_state(cx)
                .sync_state_for("")
                .sse
                .lock()
                .sse_events
                .is_none(),
            "the consumer task must take the receiver out of the shared state"
        );
    });
    visual_cx.run_until_parked();
}

// --- Connection status changes reaching the UI ---

#[gpui::test]
fn test_status_change_is_not_swallowed_by_the_event_debounce(cx: &mut TestAppContext) {
    // The worker writes the status into an Arc<Mutex<_>> gpui cannot observe,
    // so the status-change event is the only thing that repaints the windows.
    // The 500ms debounce that guards share storms must not apply to it,
    // otherwise a reconnect leaves a stale status on screen.
    setup_globals(cx);
    cx.update(|cx| {
        // Open the debounce window with the one event that owns it, so the
        // status change below really is arriving inside a live window.
        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::ShareAvailable(make_share_notification("share-1")),
            cx,
        );
        let window_opened_at = debounce_window_opened_at(cx);
        assert!(
            window_opened_at.is_some(),
            "the doorbell must have opened a debounce window for this test to mean anything"
        );

        Fulgur::handle_sse_event_for_profile(
            &test_profile_id(),
            SseEvent::ConnectionStatusChanged,
            cx,
        );

        assert_eq!(
            debounce_window_opened_at(cx),
            window_opened_at,
            "a status change must bypass the debounce instead of consuming its window"
        );
    });
}

// --- Stopping a profile's SSE connection ---

/// Time a worker thread stays alive after being asked to shut down.
///
/// Long enough that a join on the calling thread is unmistakable in the
/// timings, short enough to keep the test fast.
const STUBBORN_WORKER_LIFETIME: Duration = Duration::from_millis(700);

/// Maximum time a non-blocking stop may take before the test considers the
/// caller to have joined the worker thread inline.
const NON_BLOCKING_BUDGET: Duration = Duration::from_millis(250);

/// Install a worker that ignores the shutdown flag for `STUBBORN_WORKER_LIFETIME`.
///
/// Joining it on the calling thread costs at least that long, which is what the
/// timing assertions below detect.
fn install_stubborn_sse_worker(profile_id: &str, cx: &gpui::App) {
    let worker = Worker::spawn("test-stubborn-sse", SSE_WORKER_JOIN_TIMEOUT, |_shutdown| {
        thread::sleep(STUBBORN_WORKER_LIFETIME);
    });
    Fulgur::shared_state(cx)
        .sync_state_for(profile_id)
        .sse
        .lock()
        .worker = Some(worker);
}

#[gpui::test]
fn test_stop_sse_connection_clears_the_worker_without_blocking(cx: &mut TestAppContext) {
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        install_stubborn_sse_worker("", cx);
        *Fulgur::shared_state(cx)
            .sync_state_for("")
            .connection_status
            .lock() = SynchronizationStatus::Connected;

        let started = Instant::now();
        fulgur.update(cx, |this, cx| this.stop_sse_connection_for("", cx));
        assert!(
            started.elapsed() < NON_BLOCKING_BUDGET,
            "stopping must not join the SSE worker on the UI thread (took {:?})",
            started.elapsed()
        );

        let sync_state = Fulgur::shared_state(cx).sync_state_for("");
        assert!(
            sync_state.sse.lock().worker.is_none(),
            "the retired worker must be taken out of the shared SSE state"
        );
        assert!(
            matches!(
                *sync_state.connection_status.lock(),
                SynchronizationStatus::NotActivated
            ),
            "a stopped profile must not keep reporting a live connection"
        );
        assert!(
            sync_state.connecting_since.lock().is_none(),
            "connecting_since must be cleared when the connection is stopped"
        );
    });
}

#[gpui::test]
fn test_restart_bail_out_leaves_the_live_connection_untouched(cx: &mut TestAppContext) {
    // `prepare_sse_restart` decides whether to connect from this window's
    // settings snapshot, which is not authoritative about the live connection.
    // Its bail-out must therefore be inert: an earlier version stopped the
    // connection here and marked connected servers as no longer connected.
    let (fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        fulgur.update(cx, |this, _cx| {
            let mut profile = ServerProfile::new("Deactivated");
            profile.id = String::new();
            profile.is_active = false;
            let settings = &mut this.settings.app_settings.synchronization_settings;
            settings.is_synchronization_activated = true;
            settings.profiles.push(profile);
        });
        install_stubborn_sse_worker("", cx);
        *Fulgur::shared_state(cx)
            .sync_state_for("")
            .connection_status
            .lock() = SynchronizationStatus::Connected;

        let started = Instant::now();
        fulgur.update(cx, |this, cx| this.restart_sse_connection_for("", cx));
        assert!(
            started.elapsed() < NON_BLOCKING_BUDGET,
            "the bail-out must not join a worker on the UI thread (took {:?})",
            started.elapsed()
        );

        let sync_state = Fulgur::shared_state(cx).sync_state_for("");
        assert!(
            sync_state.sse.lock().worker.is_some(),
            "the bail-out must leave the existing worker in place"
        );
        assert!(
            matches!(
                *sync_state.connection_status.lock(),
                SynchronizationStatus::Connected
            ),
            "the bail-out must not rewrite the profile's connection status"
        );
    });
}

#[gpui::test]
fn test_sse_consumer_with_closed_channel_is_a_no_op(cx: &mut TestAppContext) {
    let (_fulgur, mut visual_cx) = setup_fulgur(cx);
    visual_cx.update(|_window, cx| {
        let tx = install_test_sse_channel(cx);
        drop(tx);
        Fulgur::spawn_sse_event_consumer("", cx);
    });
    visual_cx.run_until_parked();
    visual_cx.update(|_window, cx| {
        assert!(
            Fulgur::shared_state(cx)
                .primary_sync_state()
                .last_heartbeat
                .lock()
                .is_none(),
            "No events dispatched from closed channel"
        );
    });
}
