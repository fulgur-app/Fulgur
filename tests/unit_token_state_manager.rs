use fulgur::fulgur::sync::access_token::{TokenState, TokenStateManager};
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
