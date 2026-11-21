use fulgur::fulgur::settings::ServerProfile;
use fulgur::fulgur::sync::access_token::{
    TokenState, TokenStateManager, get_valid_token, with_valid_token,
};
use fulgur::fulgur::sync::synchronization::SynchronizationError;
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

const ONE_HOUR: Duration = Duration::from_hours(1);

#[test]
fn test_token_state_new() {
    let state = TokenState::new();
    assert!(state.access_token.is_none());
    assert!(state.token_refresh_at.is_none());
    assert!(!state.is_refreshing_token);
}

#[test]
fn test_token_state_default() {
    let state = TokenState::default();
    assert!(state.access_token.is_none());
    assert!(state.token_refresh_at.is_none());
    assert!(!state.is_refreshing_token);
}

#[test]
fn test_token_manager_new() {
    let manager = TokenStateManager::new();
    manager.clear_token(); // Should not panic
}

#[test]
fn test_token_manager_default() {
    let manager = TokenStateManager::default();
    manager.clear_token(); // Should not panic
}

#[test]
fn test_clear_token_on_empty_state() {
    let manager = TokenStateManager::new();
    // Clear on empty state should not panic
    manager.clear_token();
    manager.clear_token(); // Multiple clears should be safe
}

#[test]
fn test_clear_token_is_thread_safe() {
    let manager = Arc::new(TokenStateManager::new());
    inject_valid_token(&manager);
    let mut handles = vec![];

    // Spawn 10 threads that all try to clear the token simultaneously
    for _ in 0..10 {
        let manager_clone = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            manager_clone.clear_token();
        });
        handles.push(handle);
    }

    // Wait for all threads to complete
    for handle in handles {
        handle.join().expect("Thread should complete successfully");
    }

    assert_token_cleared(&manager);
}

// Note: Full concurrent token refresh testing requires mocking network calls.
// These tests verify the basic thread-safety of the TokenStateManager structure.

#[test]
fn test_concurrent_clear_token_calls() {
    let manager = Arc::new(TokenStateManager::new());
    let mut handles = vec![];
    let iterations = 100;

    // Every thread ends each iteration with a clear, so whichever write lands
    // last is a clear and the final state is deterministic despite contention.
    for thread_id in 0..5 {
        let manager_clone = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            for i in 0..iterations {
                inject_valid_token(&manager_clone);
                manager_clone.clear_token();
                // Add tiny sleep to increase chance of interleaving
                if (thread_id + i) % 10 == 0 {
                    thread::sleep(std::time::Duration::from_micros(1));
                }
            }
        });
        handles.push(handle);
    }

    for handle in handles {
        handle.join().expect("Thread should complete");
    }

    assert_token_cleared(&manager);

    // The manager must stay usable after the contention
    let token = inject_valid_token(&manager);
    assert_token_cached(&manager, &token);
}

#[test]
fn test_token_manager_can_be_shared_across_threads() {
    let manager = Arc::new(TokenStateManager::new());
    inject_valid_token(&manager);
    let manager1 = Arc::clone(&manager);
    let manager2 = Arc::clone(&manager);

    let handle1 = thread::spawn(move || {
        for _ in 0..50 {
            manager1.clear_token();
        }
    });

    let handle2 = thread::spawn(move || {
        for _ in 0..50 {
            manager2.clear_token();
        }
    });

    handle1.join().expect("Thread 1 should complete");
    handle2.join().expect("Thread 2 should complete");

    assert_token_cleared(&manager);
}

// These tests document the expected behavior and API usage patterns

#[test]
fn test_token_manager_usage_pattern() {
    // Document typical usage pattern
    let manager = Arc::new(TokenStateManager::new());

    // Initially, token should be clearable
    manager.clear_token();
    assert_token_cleared(&manager);

    // Manager can be cloned for use in other threads/contexts
    let manager_clone = Arc::clone(&manager);

    // Both references share the same cached token
    let token = inject_valid_token(&manager);
    assert_token_cached(&manager_clone, &token);
    manager_clone.clear_token();
    assert_token_cleared(&manager);
}

#[test]
fn test_token_state_field_access() {
    // Document the fields available in TokenState
    let mut state = TokenState::new();

    // Fields can be read and written
    assert!(state.access_token.is_none());
    state.access_token = Some("test_token".to_string());
    assert_eq!(state.access_token, Some("test_token".to_string()));

    assert!(state.token_refresh_at.is_none());
    state.token_refresh_at = Some(Instant::now() + ONE_HOUR);
    assert!(state.token_refresh_at.is_some());

    assert!(!state.is_refreshing_token);
    state.is_refreshing_token = true;
    assert!(state.is_refreshing_token);
}

// --- get_valid_token fast path and error path tests ---

fn make_http_agent() -> ureq::Agent {
    ureq::Agent::new_with_config(ureq::config::Config::builder().build())
}

/// Inject a valid token (expires 1 hour from now) into the manager.
fn inject_valid_token(manager: &TokenStateManager) -> String {
    let token = "test-jwt-token".to_string();
    manager.inject_token_for_test(token.clone(), ONE_HOUR);
    token
}

/// Assert that the manager serves `expected` from its cache
///
/// Uses an unconfigured profile, so a cache miss would fail with
/// `ServerUrlMissing` instead of reaching the network.
///
/// ### Arguments
/// - `manager`: The token manager to inspect
/// - `expected`: The token the cache must return
fn assert_token_cached(manager: &Arc<TokenStateManager>, expected: &str) {
    let profile = ServerProfile::new("Test");
    let result = get_valid_token(&profile, manager, &make_http_agent());
    assert_eq!(result.ok().as_deref(), Some(expected));
}

/// Assert that the manager holds no cached token
///
/// With an unconfigured profile, a cache miss falls through to the refresh
/// path and fails with `ServerUrlMissing` before any network I/O, which is
/// the observable proof that the token was cleared.
///
/// ### Arguments
/// - `manager`: The token manager to inspect
fn assert_token_cleared(manager: &Arc<TokenStateManager>) {
    let profile = ServerProfile::new("Test");
    let result = get_valid_token(&profile, manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expected a cleared token to require a refresh, got: {result:?}"
    );
}

#[test]
fn test_get_valid_token_returns_cached_valid_token() {
    // When a valid token is already cached, get_valid_token must return it
    // without hitting the network (fast path: returns before checking credentials).
    let manager = Arc::new(TokenStateManager::new());
    let expected_token = inject_valid_token(&manager);
    let profile = ServerProfile::new("Test"); // empty, no server_url/email
    let result = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(
        result.is_ok(),
        "Expected cached token, got: {:?}",
        result.err()
    );
    assert_eq!(result.unwrap(), expected_token);
}

#[test]
fn test_get_valid_token_fails_when_no_server_url() {
    // Without a server URL the refresh path must fail with ServerUrlMissing
    // before attempting any network I/O.
    let manager = Arc::new(TokenStateManager::new()); // no cached token
    let profile = ServerProfile::new("Test"); // server_url = None
    let result = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expected ServerUrlMissing, got: {result:?}"
    );
}

#[test]
fn test_get_valid_token_fails_when_no_email() {
    // With a server URL but no email the refresh path must fail with EmailMissing.
    let manager = Arc::new(TokenStateManager::new());
    let mut profile = ServerProfile::new("Test");
    profile.server_url = Some("https://example.com".to_string());
    let result = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::EmailMissing)),
        "Expected EmailMissing, got: {result:?}"
    );
}

#[test]
fn test_get_valid_token_after_clear_requires_refresh() {
    // After clear_token, the cached value is gone. A subsequent get_valid_token
    // must attempt a refresh, which fails with a config error when settings are empty.
    let manager = Arc::new(TokenStateManager::new());
    inject_valid_token(&manager);
    // First call should succeed using the cache.
    let profile = ServerProfile::new("Test");
    let first = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(first.is_ok(), "Pre-clear call should succeed");
    // Clear invalidates the cache.
    manager.clear_token();
    // Second call must now try to refresh and fail (no server_url).
    let second = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(
        matches!(second, Err(SynchronizationError::ServerUrlMissing)),
        "Post-clear call should require refresh and fail with ServerUrlMissing, got: {second:?}"
    );
}

#[test]
fn test_get_valid_token_resets_refreshing_flag_on_error() {
    // When a refresh fails, is_refreshing_token must be reset to false and
    // refresh_notify must be signalled. A second call on the same manager must
    // not deadlock: it should see the same error rather than waiting forever.
    let manager = Arc::new(TokenStateManager::new());
    let profile = ServerProfile::new("Test"); // no server_url → fast error
    let first = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(matches!(first, Err(SynchronizationError::ServerUrlMissing)));
    // A second call must not deadlock (flag was properly reset).
    let second = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(
        matches!(second, Err(SynchronizationError::ServerUrlMissing)),
        "Second call should fail cleanly, not deadlock: {second:?}"
    );
}

#[test]
fn test_concurrent_get_valid_token_with_cached_token() {
    // Multiple threads calling get_valid_token with a valid cached token must
    // all receive the same token value: verifying the fast path is race-free.
    let manager = Arc::new(TokenStateManager::new());
    let expected_token = inject_valid_token(&manager);
    let profile = Arc::new(ServerProfile::new("Test"));
    let mut handles = vec![];
    for _ in 0..10 {
        let m = Arc::clone(&manager);
        let p = Arc::clone(&profile);
        let expected = expected_token.clone();
        let handle = thread::spawn(move || {
            let result = get_valid_token(&p, &m, &make_http_agent());
            assert!(result.is_ok());
            assert_eq!(result.unwrap(), expected);
        });
        handles.push(handle);
    }
    for handle in handles {
        handle.join().expect("Thread should complete without panic");
    }
}

#[test]
fn test_get_valid_token_with_expired_token_falls_through_to_refresh() {
    // An expired cached token must trigger a refresh, not be returned as-is.
    // The refresh fails with ServerUrlMissing when unconfigured.
    let manager = Arc::new(TokenStateManager::new());
    manager.inject_token_for_test("old-expired-token".to_string(), Duration::ZERO);
    let profile = ServerProfile::new("Test");
    let result = get_valid_token(&profile, &manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expired token must trigger refresh and fail with ServerUrlMissing, got: {result:?}"
    );
}

#[test]
fn test_get_valid_token_reuses_a_token_shorter_than_the_five_minute_buffer() {
    // Fulgurant allows lifetimes down to 60 seconds. A fixed 5-minute buffer
    // made such tokens stale on arrival, so every call minted a new one.
    let manager = Arc::new(TokenStateManager::new());
    manager.inject_token_for_test("short-lived-token".to_string(), Duration::from_secs(120));
    assert_token_cached(&manager, "short-lived-token");
}

#[test]
fn test_invalidate_token_drops_the_rejected_token() {
    let manager = Arc::new(TokenStateManager::new());
    let token = inject_valid_token(&manager);
    manager.invalidate_token(&token);
    assert_token_cleared(&manager);
}

#[test]
fn test_invalidate_token_keeps_a_token_minted_since_the_rejection() {
    // Another thread already replaced the rejected token: its late 401 must
    // not discard the fresh one.
    let manager = Arc::new(TokenStateManager::new());
    let fresh_token = inject_valid_token(&manager);
    manager.invalidate_token("stale-token");
    assert_token_cached(&manager, &fresh_token);
}

#[test]
fn test_with_valid_token_refreshes_after_a_cached_token_is_rejected() {
    // The cached token is refused, so it is invalidated and a new one is
    // requested. The unconfigured profile makes that refresh fail with
    // ServerUrlMissing, proving the retry went back to the token endpoint.
    let manager = Arc::new(TokenStateManager::new());
    let token = inject_valid_token(&manager);
    let profile = ServerProfile::new("Test");
    let mut seen_tokens = Vec::new();
    let result: Result<(), _> =
        with_valid_token(&profile, &manager, &make_http_agent(), |bearer| {
            seen_tokens.push(bearer.to_string());
            Err(SynchronizationError::AuthenticationFailed)
        });
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expected a refresh attempt after the rejection, got: {result:?}"
    );
    assert_eq!(seen_tokens, vec![token]);
    assert_token_cleared(&manager);
}

#[test]
fn test_with_valid_token_keeps_the_cache_on_other_errors() {
    let manager = Arc::new(TokenStateManager::new());
    let token = inject_valid_token(&manager);
    let profile = ServerProfile::new("Test");
    let mut attempts = 0;
    let result: Result<(), _> = with_valid_token(&profile, &manager, &make_http_agent(), |_| {
        attempts += 1;
        Err(SynchronizationError::ServerError(500))
    });
    assert!(matches!(
        result,
        Err(SynchronizationError::ServerError(500))
    ));
    assert_eq!(attempts, 1, "Only an authentication failure is retried");
    assert_token_cached(&manager, &token);
}

#[test]
fn test_with_valid_token_returns_the_request_result() {
    let manager = Arc::new(TokenStateManager::new());
    let token = inject_valid_token(&manager);
    let profile = ServerProfile::new("Test");
    let result = with_valid_token(&profile, &manager, &make_http_agent(), |bearer| {
        Ok(bearer.len())
    });
    assert_eq!(result.ok(), Some(token.len()));
}
