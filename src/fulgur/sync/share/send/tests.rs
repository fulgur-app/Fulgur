use super::{MAX_SYNC_SHARE_PAYLOAD_BYTES, share_file};
use crate::fulgur::settings::ServerProfile;
use crate::fulgur::sync::share::{Device, ShareFileRequest};
use crate::fulgur::sync::{access_token::TokenStateManager, synchronization::SynchronizationError};
use crate::fulgur::utils::crypto_helper::generate_key_pair;
use std::sync::Arc;

fn make_http_agent() -> ureq::Agent {
    ureq::Agent::new_with_config(ureq::config::Config::builder().build())
}

/// Build a `ServerProfile` whose server URL points at a port that
/// is guaranteed to refuse connections immediately (no real network needed).
fn make_profile_with_server_url() -> ServerProfile {
    let mut p = ServerProfile::new("Test");
    p.server_url = Some("http://127.0.0.1:19999".to_string());
    p
}

fn make_token_manager_with_valid_token() -> Arc<TokenStateManager> {
    let manager = Arc::new(TokenStateManager::new());
    let expires_at = time::OffsetDateTime::now_utc() + time::Duration::hours(1);
    manager.inject_token_for_test("test-jwt-token".to_string(), expires_at);
    manager
}

fn make_device(id: &str, device_type: &str, public_key: Option<&str>) -> Device {
    Device {
        id: id.to_string(),
        name: format!("{id}-name"),
        device_type: device_type.to_string(),
        public_key: public_key.map(str::to_string),
        created_at: "2024-01-01T00:00:00Z".to_string(),
        expires_at: "2025-01-01T00:00:00Z".to_string(),
    }
}

fn make_basic_request(device_id: &str) -> ShareFileRequest {
    ShareFileRequest {
        content: Arc::from("hello world"),
        file_name: "test.txt".to_string(),
        device_ids: vec![device_id.to_string()],
        file_path: None,
    }
}

// ========== share_file content size guards ==========

#[test]
fn test_share_file_rejects_content_larger_than_limit() {
    let mut profile = ServerProfile::new("Test");
    profile.server_url = Some("https://example.com".to_string());

    let request = ShareFileRequest {
        content: Arc::from("A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES + 1)),
        file_name: "large.txt".to_string(),
        device_ids: vec!["device-1".to_string()],
        file_path: None,
    };

    let result = share_file(
        &profile,
        &request,
        &[],
        &Arc::new(TokenStateManager::new()),
        &ureq::Agent::new_with_config(ureq::config::Config::default()),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    );

    assert!(matches!(
        result,
        Err(SynchronizationError::ContentTooLarge { .. })
    ));
}

#[test]
fn test_share_file_accepts_content_at_exact_limit() {
    let mut profile = ServerProfile::new("Test");
    profile.server_url = Some("https://example.com".to_string());
    // Keep email unset so the flow fails deterministically at validation/network setup,
    // after content-size checks have already passed.
    profile.email = None;

    let request = ShareFileRequest {
        content: Arc::from("A".repeat(MAX_SYNC_SHARE_PAYLOAD_BYTES)),
        file_name: "max-size.txt".to_string(),
        device_ids: vec!["device-1".to_string()],
        file_path: None,
    };

    let result = share_file(
        &profile,
        &request,
        &[],
        &Arc::new(TokenStateManager::new()),
        &ureq::Agent::new_with_config(ureq::config::Config::default()),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    );

    assert!(
        !matches!(result, Err(SynchronizationError::ContentTooLarge { .. })),
        "Payload at exact limit should not be rejected as too large"
    );
}

// ========== share_file validation guards ==========

#[test]
fn test_share_file_rejects_missing_server_url() {
    let profile = ServerProfile::new("Test"); // server_url = None
    let request = ShareFileRequest {
        content: Arc::from("hello"),
        file_name: "test.txt".to_string(),
        device_ids: vec!["device-1".to_string()],
        file_path: None,
    };
    let result = share_file(
        &profile,
        &request,
        &[],
        &Arc::new(TokenStateManager::new()),
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    );
    assert!(
        matches!(result, Err(SynchronizationError::ServerUrlMissing)),
        "Expected ServerUrlMissing, got: {:?}",
        result.err()
    );
}

#[test]
fn test_share_file_rejects_empty_content() {
    let mut profile = ServerProfile::new("Test");
    profile.server_url = Some("https://example.com".to_string());
    let request = ShareFileRequest {
        content: Arc::from(""),
        file_name: "test.txt".to_string(),
        device_ids: vec!["device-1".to_string()],
        file_path: None,
    };
    let result = share_file(
        &profile,
        &request,
        &[],
        &Arc::new(TokenStateManager::new()),
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    );
    assert!(
        matches!(result, Err(SynchronizationError::ContentMissing)),
        "Expected ContentMissing, got: {:?}",
        result.err()
    );
}

#[test]
fn test_share_file_rejects_empty_file_name() {
    let mut profile = ServerProfile::new("Test");
    profile.server_url = Some("https://example.com".to_string());
    let request = ShareFileRequest {
        content: Arc::from("hello"),
        file_name: String::new(),
        device_ids: vec!["device-1".to_string()],
        file_path: None,
    };
    let result = share_file(
        &profile,
        &request,
        &[],
        &Arc::new(TokenStateManager::new()),
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    );
    assert!(
        matches!(result, Err(SynchronizationError::FileNameMissing)),
        "Expected FileNameMissing, got: {:?}",
        result.err()
    );
}

#[test]
fn test_share_file_rejects_empty_device_ids() {
    let mut profile = ServerProfile::new("Test");
    profile.server_url = Some("https://example.com".to_string());
    let request = ShareFileRequest {
        content: Arc::from("hello"),
        file_name: "test.txt".to_string(),
        device_ids: vec![],
        file_path: None,
    };
    let result = share_file(
        &profile,
        &request,
        &[],
        &Arc::new(TokenStateManager::new()),
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    );
    assert!(
        matches!(result, Err(SynchronizationError::DeviceIdsMissing)),
        "Expected DeviceIdsMissing, got: {:?}",
        result.err()
    );
}

// ========== share_file per-device execution paths ==========
// These tests inject a valid cached token to bypass the network token-refresh
// and exercise the per-device branches inside the scoped thread pool.

#[test]
fn test_share_file_fails_for_unknown_device_id() {
    // Upfront validation: an unknown device ID must fail the whole request.
    let profile = make_profile_with_server_url();
    let token_manager = make_token_manager_with_valid_token();
    let err = share_file(
        &profile,
        &make_basic_request("nonexistent-device"),
        &[], // no devices provided
        &token_manager,
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    )
    .expect_err("share_file should fail for unknown device ID");
    assert!(
        matches!(err, SynchronizationError::Other(_)),
        "Unknown device should produce Other error, got: {err:?}"
    );
}

#[test]
fn test_share_file_fails_when_device_has_no_public_key() {
    // Upfront validation: a device with no public key must fail before any work.
    let profile = make_profile_with_server_url();
    let token_manager = make_token_manager_with_valid_token();
    let device = make_device("device-no-key", "desktop", None);
    let err = share_file(
        &profile,
        &make_basic_request("device-no-key"),
        &[device],
        &token_manager,
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    )
    .expect_err("share_file should fail when device has no public key");
    assert!(
        matches!(err, SynchronizationError::MissingPublicKey(_)),
        "Device without public key should produce MissingPublicKey, got: {err:?}"
    );
}

#[test]
fn test_share_file_fails_when_device_has_invalid_public_key() {
    // Upfront validation: a malformed key must fail before encryption is attempted.
    let profile = make_profile_with_server_url();
    let token_manager = make_token_manager_with_valid_token();
    let device = make_device(
        "device-bad-key",
        "laptop",
        Some("not-a-valid-age-public-key"),
    );
    let err = share_file(
        &profile,
        &make_basic_request("device-bad-key"),
        &[device],
        &token_manager,
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    )
    .expect_err("share_file should fail for a device with an invalid public key");
    assert!(
        matches!(err, SynchronizationError::InvalidPublicKey(_)),
        "Invalid public key should produce InvalidPublicKey, got: {err:?}"
    );
}

#[test]
fn test_share_file_with_valid_device_key_fails_at_network() {
    // Encryption succeeds; the failure comes from the network call to a
    // non-listening port (ConnectionRefused, immediate).
    let profile = make_profile_with_server_url(); // 127.0.0.1:19999
    let token_manager = make_token_manager_with_valid_token();
    let (_, public_key) = generate_key_pair();
    let device = make_device("device-valid-key", "server", Some(&public_key.to_string()));
    let result = share_file(
        &profile,
        &make_basic_request("device-valid-key"),
        &[device],
        &token_manager,
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    )
    .expect("share_file returns Ok(ShareResult) even when per-device send fails");
    assert_eq!(result.successes.len(), 0);
    assert_eq!(result.failures.len(), 1);
    assert_eq!(result.failures[0].0, "device-valid-key");
    // The error must be network-level, not a validation or crypto error.
    assert!(
        !matches!(
            result.failures[0].1,
            SynchronizationError::EncryptionFailed
                | SynchronizationError::MissingPublicKey(_)
                | SynchronizationError::InvalidPublicKey(_)
                | SynchronizationError::ContentMissing
                | SynchronizationError::ContentTooLarge { .. }
        ),
        "Expected a network-level error, got: {:?}",
        result.failures[0].1
    );
}

#[test]
fn test_share_file_fails_fast_on_first_invalid_device() {
    // Upfront validation iterates device_ids in order; the first invalid one
    // fails the whole request before any compression or encryption occurs.
    let profile = make_profile_with_server_url();
    let token_manager = make_token_manager_with_valid_token();
    let device_no_key = make_device("device-no-key", "desktop", None);
    let request = ShareFileRequest {
        content: Arc::from("shared content"),
        file_name: "multi.txt".to_string(),
        device_ids: vec!["device-missing".to_string(), "device-no-key".to_string()],
        file_path: None,
    };
    let err = share_file(
        &profile,
        &request,
        &[device_no_key],
        &token_manager,
        &make_http_agent(),
        MAX_SYNC_SHARE_PAYLOAD_BYTES as u64,
    )
    .expect_err("share_file should fail on the first invalid device");
    assert!(
        matches!(err, SynchronizationError::Other(_)),
        "First unknown device should produce Other error, got: {err:?}"
    );
}
