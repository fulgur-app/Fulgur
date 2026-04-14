//! Unit tests for SSE lifecycle, status transitions, and shutdown behaviour.
//!
//! Event *parsing* is already covered by `unit_sse_event_parsing.rs`.
//! This file focuses on the structural and threading aspects:
//! - `SseState` construction
//! - `SynchronizationStatus` transitions (`is_connected`, `from_error`)
//! - `set_sync_server_connection_status` atomic update
//! - `connect_sse` immediate error when server URL is missing
//! - Shutdown-flag-first exit: thread exits in the first loop iteration

use fulgur::fulgur::settings::SynchronizationSettings;
use fulgur::fulgur::sync::access_token::TokenStateManager;
use fulgur::fulgur::sync::sse::{SseEvent, SseState, connect_sse};
use fulgur::fulgur::sync::synchronization::{
    SynchronizationError, SynchronizationStatus, set_sync_server_connection_status,
};
use fulgur_common::api::shares::SharedFileResponse;
use parking_lot::Mutex;
use std::sync::{Arc, atomic::AtomicBool};
use std::time::{Duration, Instant};

fn make_pending_shared_files() -> Arc<Mutex<Vec<SharedFileResponse>>> {
    Arc::new(Mutex::new(Vec::new()))
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn make_http_agent() -> ureq::Agent {
    ureq::Agent::new_with_config(ureq::config::Config::builder().build())
}

fn make_connection_status(initial: SynchronizationStatus) -> Arc<Mutex<SynchronizationStatus>> {
    Arc::new(Mutex::new(initial))
}

fn settings_with_server_url() -> SynchronizationSettings {
    let mut s = SynchronizationSettings::new();
    s.server_url = Some("https://example.com".to_string());
    s
}

// ---------------------------------------------------------------------------
// SseState construction
// ---------------------------------------------------------------------------

#[test]
fn test_sse_state_new_all_fields_none() {
    let state = SseState::new();
    assert!(state.sse_events.is_none());
    assert!(state.sse_event_tx.is_none());
    assert!(state.sse_shutdown_flag.is_none());
    assert!(state.last_sse_event.is_none());
    assert!(state.sse_thread_handle.lock().is_none());
}

#[test]
fn test_sse_state_default_matches_new() {
    let a = SseState::new();
    let b = SseState::default();
    // Both should be in the same empty state.
    assert!(a.sse_events.is_none() && b.sse_events.is_none());
    assert!(a.sse_shutdown_flag.is_none() && b.sse_shutdown_flag.is_none());
    assert!(a.last_sse_event.is_none() && b.last_sse_event.is_none());
}

#[test]
fn test_sse_state_thread_handle_is_arc_shared() {
    // The Arc around the thread handle allows other threads to store the handle
    // once connect_sse has returned. Verify the Arc can be cloned and the lock shared.
    let state = SseState::new();
    let handle_clone = Arc::clone(&state.sse_thread_handle);
    assert!(state.sse_thread_handle.lock().is_none());
    assert!(handle_clone.lock().is_none());
}

// ---------------------------------------------------------------------------
// SynchronizationStatus — is_connected
// ---------------------------------------------------------------------------

#[test]
fn test_sync_status_connected_is_connected() {
    assert!(SynchronizationStatus::Connected.is_connected());
}

#[test]
fn test_sync_status_connecting_is_not_connected() {
    assert!(!SynchronizationStatus::Connecting.is_connected());
}

#[test]
fn test_sync_status_disconnected_is_not_connected() {
    assert!(!SynchronizationStatus::Disconnected.is_connected());
}

#[test]
fn test_sync_status_authentication_failed_is_not_connected() {
    assert!(!SynchronizationStatus::AuthenticationFailed.is_connected());
}

#[test]
fn test_sync_status_connection_failed_is_not_connected() {
    assert!(!SynchronizationStatus::ConnectionFailed.is_connected());
}

#[test]
fn test_sync_status_other_is_not_connected() {
    assert!(!SynchronizationStatus::Other.is_connected());
}

#[test]
fn test_sync_status_not_activated_is_not_connected() {
    assert!(!SynchronizationStatus::NotActivated.is_connected());
}

// ---------------------------------------------------------------------------
// SynchronizationStatus — from_error
// ---------------------------------------------------------------------------

#[test]
fn test_sync_status_from_error_auth_failed() {
    let status = SynchronizationStatus::from_error(&SynchronizationError::AuthenticationFailed);
    assert!(matches!(
        status,
        SynchronizationStatus::AuthenticationFailed
    ));
}

#[test]
fn test_sync_status_from_error_host_not_found() {
    let status = SynchronizationStatus::from_error(&SynchronizationError::HostNotFound);
    assert!(matches!(status, SynchronizationStatus::ConnectionFailed));
}

#[test]
fn test_sync_status_from_error_connection_failed() {
    let status = SynchronizationStatus::from_error(&SynchronizationError::ConnectionFailed);
    assert!(matches!(status, SynchronizationStatus::ConnectionFailed));
}

#[test]
fn test_sync_status_from_error_timeout() {
    let status =
        SynchronizationStatus::from_error(&SynchronizationError::Timeout("30s".to_string()));
    assert!(matches!(status, SynchronizationStatus::ConnectionFailed));
}

#[test]
fn test_sync_status_from_error_server_url_missing_maps_to_other() {
    // Errors that don't map to a specific status fall through to Other.
    let status = SynchronizationStatus::from_error(&SynchronizationError::ServerUrlMissing);
    assert!(matches!(status, SynchronizationStatus::Other));
}

#[test]
fn test_sync_status_from_error_email_missing_maps_to_other() {
    let status = SynchronizationStatus::from_error(&SynchronizationError::EmailMissing);
    assert!(matches!(status, SynchronizationStatus::Other));
}

#[test]
fn test_sync_status_from_error_server_error_maps_to_other() {
    let status = SynchronizationStatus::from_error(&SynchronizationError::ServerError(500));
    assert!(matches!(status, SynchronizationStatus::Other));
}

#[test]
fn test_sync_status_from_error_other_maps_to_other() {
    let status =
        SynchronizationStatus::from_error(&SynchronizationError::Other("unknown".to_string()));
    assert!(matches!(status, SynchronizationStatus::Other));
}

// ---------------------------------------------------------------------------
// set_sync_server_connection_status
// ---------------------------------------------------------------------------

#[test]
fn test_set_sync_server_connection_status_updates_value() {
    let status = make_connection_status(SynchronizationStatus::Disconnected);
    assert!(!status.lock().is_connected());

    set_sync_server_connection_status(Arc::clone(&status), SynchronizationStatus::Connected);

    assert!(status.lock().is_connected());
}

#[test]
fn test_set_sync_server_connection_status_can_cycle() {
    let status = make_connection_status(SynchronizationStatus::Connected);

    set_sync_server_connection_status(Arc::clone(&status), SynchronizationStatus::Disconnected);
    assert!(!status.lock().is_connected());

    set_sync_server_connection_status(Arc::clone(&status), SynchronizationStatus::Connected);
    assert!(status.lock().is_connected());
}

#[test]
fn test_set_sync_server_connection_status_thread_safe() {
    use std::thread;
    let status = make_connection_status(SynchronizationStatus::Disconnected);
    let mut handles = vec![];

    for _ in 0..5 {
        let s = Arc::clone(&status);
        handles.push(thread::spawn(move || {
            set_sync_server_connection_status(s, SynchronizationStatus::Connected);
        }));
    }
    for h in handles {
        h.join().expect("Thread should complete");
    }
    assert!(status.lock().is_connected());
}

// ---------------------------------------------------------------------------
// connect_sse — error path (no server URL)
// ---------------------------------------------------------------------------

#[test]
fn test_connect_sse_fails_without_server_url() {
    // connect_sse must return Err(ServerUrlMissing) immediately and not spawn a thread.
    let settings = SynchronizationSettings::new(); // server_url = None
    let (tx, _rx) = std::sync::mpsc::channel();
    let shutdown_flag = Arc::new(AtomicBool::new(false));
    let status = make_connection_status(SynchronizationStatus::Disconnected);
    let token_manager = Arc::new(TokenStateManager::new());
    let http_agent = Arc::new(make_http_agent());

    let result = connect_sse(
        &settings,
        tx,
        shutdown_flag,
        status,
        token_manager,
        http_agent,
        make_pending_shared_files(),
    );

    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expected ServerUrlMissing, got: {:?}",
        result.err()
    );
}

// ---------------------------------------------------------------------------
// connect_sse — shutdown-flag-first exit
// ---------------------------------------------------------------------------

#[test]
fn test_connect_sse_exits_immediately_when_shutdown_pre_set() {
    // Setting the shutdown flag to true before calling connect_sse means the
    // spawned thread checks the flag at the top of its loop and exits without
    // performing any network I/O or sleeping.
    let settings = settings_with_server_url();
    let (tx, _rx) = std::sync::mpsc::channel();
    let shutdown_flag = Arc::new(AtomicBool::new(true)); // pre-set
    let status = make_connection_status(SynchronizationStatus::Disconnected);
    let token_manager = Arc::new(TokenStateManager::new());
    let http_agent = Arc::new(make_http_agent());

    let handle = connect_sse(
        &settings,
        tx,
        shutdown_flag,
        status,
        token_manager,
        http_agent,
        make_pending_shared_files(),
    )
    .expect("connect_sse should succeed with a server URL");

    // The thread should exit in the first iteration — no sleep, no network.
    // Give it a generous deadline to avoid flakiness on slow CI runners.
    let deadline = Instant::now() + Duration::from_secs(2);
    while !handle.is_finished() && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(10));
    }

    assert!(
        handle.is_finished(),
        "SSE thread should have exited within 2 seconds after shutdown flag was pre-set"
    );
    handle.join().expect("Thread should exit cleanly");
}

#[test]
fn test_connect_sse_returns_ok_handle_with_valid_settings() {
    // Verify that connect_sse returns Ok(JoinHandle) when the server URL is present,
    // regardless of whether the connection succeeds later.
    let settings = settings_with_server_url();
    let (tx, _rx) = std::sync::mpsc::channel();
    let shutdown_flag = Arc::new(AtomicBool::new(true)); // shut down immediately
    let status = make_connection_status(SynchronizationStatus::Disconnected);
    let token_manager = Arc::new(TokenStateManager::new());
    let http_agent = Arc::new(make_http_agent());

    let result = connect_sse(
        &settings,
        tx,
        shutdown_flag,
        status,
        token_manager,
        http_agent,
        make_pending_shared_files(),
    );

    assert!(
        result.is_ok(),
        "Expected Ok(JoinHandle), got: {:?}",
        result.err()
    );
    let handle = result.unwrap();

    // Clean up: wait for the pre-shutdown thread to exit.
    let _ = handle.join();
}

// ---------------------------------------------------------------------------
// SseEvent — Debug / Clone sanity checks
// ---------------------------------------------------------------------------

#[test]
fn test_sse_event_heartbeat_can_be_cloned() {
    let event = SseEvent::Heartbeat {
        timestamp: "2024-01-15T12:00:00Z".to_string(),
    };
    let cloned = event.clone();
    assert!(matches!(cloned, SseEvent::Heartbeat { .. }));
}

#[test]
fn test_sse_event_error_can_be_cloned() {
    let event = SseEvent::Error("connection refused".to_string());
    let cloned = event.clone();
    match cloned {
        SseEvent::Error(msg) => assert_eq!(msg, "connection refused"),
        _ => panic!("Expected Error event"),
    }
}

#[test]
fn test_sse_event_debug_output_non_empty() {
    let event = SseEvent::Heartbeat {
        timestamp: "ts".to_string(),
    };
    let debug = format!("{:?}", event);
    assert!(!debug.is_empty());
}
