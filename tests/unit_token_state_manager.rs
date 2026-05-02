use fulgur::fulgur::settings::SynchronizationSettings;
use fulgur::fulgur::sync::access_token::{TokenState, TokenStateManager, get_valid_token};
use fulgur::fulgur::sync::synchronization::SynchronizationError;
use std::sync::Arc;
use std::thread;
use time::OffsetDateTime;

#[test]
fn test_token_state_new() {
    let state = TokenState::new();
    assert!(state.access_token.is_none());
    assert!(state.token_expires_at.is_none());
    assert!(!state.is_refreshing_token);
}

#[test]
fn test_token_state_default() {
    let state = TokenState::default();
    assert!(state.access_token.is_none());
    assert!(state.token_expires_at.is_none());
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

    // No assertion needed - if we got here without deadlock/panic, test passes
}

// Note: Full concurrent token refresh testing requires mocking network calls.
// These tests verify the basic thread-safety of the TokenStateManager structure.

#[test]
fn test_concurrent_clear_token_calls() {
    let manager = Arc::new(TokenStateManager::new());
    let mut handles = vec![];
    let iterations = 100;

    for thread_id in 0..5 {
        let manager_clone = Arc::clone(&manager);
        let handle = thread::spawn(move || {
            for i in 0..iterations {
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

    // Verify manager is still functional after concurrent access
    manager.clear_token();
}

#[test]
fn test_token_manager_can_be_shared_across_threads() {
    let manager = Arc::new(TokenStateManager::new());
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

    // If we got here, Arc<TokenStateManager> is Send + Sync
}

// These tests document the expected behavior and API usage patterns

#[test]
fn test_token_manager_usage_pattern() {
    // Document typical usage pattern
    let manager = Arc::new(TokenStateManager::new());

    // Initially, token should be clearable
    manager.clear_token();

    // Manager can be cloned for use in other threads/contexts
    let manager_clone = Arc::clone(&manager);

    // Both references work independently
    manager.clear_token();
    manager_clone.clear_token();
}

#[test]
fn test_token_state_field_access() {
    // Document the fields available in TokenState
    let mut state = TokenState::new();

    // Fields can be read and written
    assert!(state.access_token.is_none());
    state.access_token = Some("test_token".to_string());
    assert_eq!(state.access_token, Some("test_token".to_string()));

    assert!(state.token_expires_at.is_none());
    let expires = OffsetDateTime::now_utc() + time::Duration::hours(1);
    state.token_expires_at = Some(expires);
    assert!(state.token_expires_at.is_some());

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
    let expires_at = OffsetDateTime::now_utc() + time::Duration::hours(1);
    manager.inject_token_for_test(token.clone(), expires_at);
    token
}

#[test]
fn test_get_valid_token_returns_cached_valid_token() {
    // When a valid token is already cached, get_valid_token must return it
    // without hitting the network (fast path: returns before checking credentials).
    let manager = Arc::new(TokenStateManager::new());
    let expected_token = inject_valid_token(&manager);
    let settings = SynchronizationSettings::new(); // empty, no server_url/email
    let result = get_valid_token(&settings, &manager, &make_http_agent());
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
    let settings = SynchronizationSettings::new(); // server_url = None
    let result = get_valid_token(&settings, &manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expected ServerUrlMissing, got: {result:?}"
    );
}

#[test]
fn test_get_valid_token_fails_when_no_email() {
    // With a server URL but no email the refresh path must fail with EmailMissing.
    let manager = Arc::new(TokenStateManager::new());
    let mut settings = SynchronizationSettings::new();
    settings.server_url = Some("https://example.com".to_string());
    let result = get_valid_token(&settings, &manager, &make_http_agent());
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
    let settings = SynchronizationSettings::new();
    let first = get_valid_token(&settings, &manager, &make_http_agent());
    assert!(first.is_ok(), "Pre-clear call should succeed");
    // Clear invalidates the cache.
    manager.clear_token();
    // Second call must now try to refresh and fail (no server_url).
    let second = get_valid_token(&settings, &manager, &make_http_agent());
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
    let settings = SynchronizationSettings::new(); // no server_url → fast error
    let first = get_valid_token(&settings, &manager, &make_http_agent());
    assert!(matches!(first, Err(SynchronizationError::ServerUrlMissing)));
    // A second call must not deadlock (flag was properly reset).
    let second = get_valid_token(&settings, &manager, &make_http_agent());
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
    let settings = Arc::new(SynchronizationSettings::new());
    let mut handles = vec![];
    for _ in 0..10 {
        let m = Arc::clone(&manager);
        let s = Arc::clone(&settings);
        let expected = expected_token.clone();
        let handle = thread::spawn(move || {
            let result = get_valid_token(&s, &m, &make_http_agent());
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
    // An expired cached token (beyond the 5-minute buffer) must trigger a refresh,
    // not be returned as-is. The refresh fails with ServerUrlMissing when unconfigured.
    let manager = Arc::new(TokenStateManager::new());
    let expired_at = OffsetDateTime::now_utc() - time::Duration::minutes(10);
    manager.inject_token_for_test("old-expired-token".to_string(), expired_at);
    let settings = SynchronizationSettings::new();
    let result = get_valid_token(&settings, &manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expired token must trigger refresh and fail with ServerUrlMissing, got: {result:?}"
    );
}

#[test]
fn test_get_valid_token_with_near_expiry_token_falls_through_to_refresh() {
    // A token expiring within the 5-minute buffer must also trigger a refresh.
    let manager = Arc::new(TokenStateManager::new());
    let near_expiry = OffsetDateTime::now_utc() + time::Duration::minutes(3);
    manager.inject_token_for_test("near-expiry-token".to_string(), near_expiry);
    let settings = SynchronizationSettings::new();
    let result = get_valid_token(&settings, &manager, &make_http_agent());
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Near-expiry token must trigger refresh, got: {result:?}"
    );
}
