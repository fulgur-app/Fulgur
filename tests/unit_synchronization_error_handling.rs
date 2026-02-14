use fulgur::fulgur::sync::synchronization::{
    SynchronizationError, SynchronizationStatus, handle_ureq_error,
};
use std::io;

// Helper to create mock ureq errors for testing
fn mock_io_error(kind: io::ErrorKind) -> ureq::Error {
    ureq::Error::Io(io::Error::new(kind, "mock error"))
}

#[test]
fn test_handle_ureq_error_auth_failed_401() {
    let error = ureq::Error::StatusCode(401);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::AuthenticationFailed));
}

#[test]
fn test_handle_ureq_error_auth_failed_403() {
    let error = ureq::Error::StatusCode(403);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::AuthenticationFailed));
}

#[test]
fn test_handle_ureq_error_bad_request_400() {
    let error = ureq::Error::StatusCode(400);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::BadRequest));
}

#[test]
fn test_handle_ureq_error_server_error_500() {
    let error = ureq::Error::StatusCode(500);
    let result = handle_ureq_error(error, "Test context");
    match result {
        SynchronizationError::ServerError(code) => assert_eq!(code, 500),
        _ => panic!("Expected ServerError"),
    }
}

#[test]
fn test_handle_ureq_error_server_error_503() {
    let error = ureq::Error::StatusCode(503);
    let result = handle_ureq_error(error, "Test context");
    match result {
        SynchronizationError::ServerError(code) => assert_eq!(code, 503),
        _ => panic!("Expected ServerError"),
    }
}

#[test]
fn test_handle_ureq_error_io_connection_refused() {
    let error = mock_io_error(io::ErrorKind::ConnectionRefused);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::ConnectionFailed));
}

#[test]
fn test_handle_ureq_error_io_timed_out() {
    let error = mock_io_error(io::ErrorKind::TimedOut);
    let result = handle_ureq_error(error, "Test context");
    match result {
        SynchronizationError::Timeout(msg) => assert!(msg.contains("mock error")),
        _ => panic!("Expected Timeout"),
    }
}

#[test]
fn test_handle_ureq_error_io_connection_reset() {
    let error = mock_io_error(io::ErrorKind::ConnectionReset);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::ConnectionFailed));
}

#[test]
fn test_handle_ureq_error_io_connection_aborted() {
    let error = mock_io_error(io::ErrorKind::ConnectionAborted);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::ConnectionFailed));
}

#[test]
fn test_handle_ureq_error_io_addr_not_available() {
    let error = mock_io_error(io::ErrorKind::AddrNotAvailable);
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::HostNotFound));
}

#[test]
fn test_handle_ureq_error_io_other_kind() {
    let error = mock_io_error(io::ErrorKind::PermissionDenied);
    let result = handle_ureq_error(error, "Test context");
    match result {
        SynchronizationError::Other(msg) => assert!(msg.contains("mock error")),
        _ => panic!("Expected Other"),
    }
}

#[test]
fn test_handle_ureq_error_connection_failed() {
    let error = ureq::Error::ConnectionFailed;
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::ConnectionFailed));
}

#[test]
fn test_handle_ureq_error_host_not_found() {
    let error = ureq::Error::HostNotFound;
    let result = handle_ureq_error(error, "Test context");
    assert!(matches!(result, SynchronizationError::HostNotFound));
}

#[test]
fn test_sync_status_from_error_auth_failed() {
    let error = SynchronizationError::AuthenticationFailed;
    let status = SynchronizationStatus::from_error(&error);
    assert!(matches!(
        status,
        SynchronizationStatus::AuthenticationFailed
    ));
}

#[test]
fn test_sync_status_from_error_host_not_found() {
    let error = SynchronizationError::HostNotFound;
    let status = SynchronizationStatus::from_error(&error);
    assert!(matches!(status, SynchronizationStatus::ConnectionFailed));
}

#[test]
fn test_sync_status_from_error_connection_failed() {
    let error = SynchronizationError::ConnectionFailed;
    let status = SynchronizationStatus::from_error(&error);
    assert!(matches!(status, SynchronizationStatus::ConnectionFailed));
}

#[test]
fn test_sync_status_from_error_timeout() {
    let error = SynchronizationError::Timeout("test".to_string());
    let status = SynchronizationStatus::from_error(&error);
    assert!(matches!(status, SynchronizationStatus::ConnectionFailed));
}

#[test]
fn test_sync_status_from_error_other_variants() {
    // All other error types should map to SynchronizationStatus::Other
    let test_cases = vec![
        SynchronizationError::BadRequest,
        SynchronizationError::CompressionFailed,
        SynchronizationError::ContentMissing,
        SynchronizationError::ContentTooLarge,
        SynchronizationError::DeviceIdsMissing,
        SynchronizationError::DeviceKeyMissing,
        SynchronizationError::EmailMissing,
        SynchronizationError::EncryptionFailed,
        SynchronizationError::FileNameMissing,
        SynchronizationError::InvalidResponse("test".to_string()),
        SynchronizationError::MissingEncryptionKey,
        SynchronizationError::MissingExpirationDate,
        SynchronizationError::MissingPublicKey("device1".to_string()),
        SynchronizationError::Other("test".to_string()),
        SynchronizationError::ServerError(500),
        SynchronizationError::ServerUrlMissing,
    ];

    for error in test_cases {
        let status = SynchronizationStatus::from_error(&error);
        assert!(
            matches!(status, SynchronizationStatus::Other),
            "Error {:?} should map to SynchronizationStatus::Other",
            error
        );
    }
}

#[test]
fn test_sync_status_is_connected_true() {
    let status = SynchronizationStatus::Connected;
    assert!(status.is_connected());
}

#[test]
fn test_sync_status_is_connected_false_disconnected() {
    let status = SynchronizationStatus::Disconnected;
    assert!(!status.is_connected());
}

#[test]
fn test_sync_status_is_connected_false_auth_failed() {
    let status = SynchronizationStatus::AuthenticationFailed;
    assert!(!status.is_connected());
}

#[test]
fn test_sync_status_is_connected_false_connection_failed() {
    let status = SynchronizationStatus::ConnectionFailed;
    assert!(!status.is_connected());
}

#[test]
fn test_sync_status_is_connected_false_other() {
    let status = SynchronizationStatus::Other;
    assert!(!status.is_connected());
}

#[test]
fn test_sync_status_is_connected_false_not_activated() {
    let status = SynchronizationStatus::NotActivated;
    assert!(!status.is_connected());
}

// ============================================================================
// Tests for SynchronizationError Display implementation
// ============================================================================

#[test]
fn test_sync_error_display_auth_failed() {
    let error = SynchronizationError::AuthenticationFailed;
    assert_eq!(error.to_string(), "Authentication failed");
}

#[test]
fn test_sync_error_display_bad_request() {
    let error = SynchronizationError::BadRequest;
    assert_eq!(error.to_string(), "Bad request");
}

#[test]
fn test_sync_error_display_connection_failed() {
    let error = SynchronizationError::ConnectionFailed;
    assert_eq!(error.to_string(), "Cannot connect to sync server");
}

#[test]
fn test_sync_error_display_host_not_found() {
    let error = SynchronizationError::HostNotFound;
    assert_eq!(error.to_string(), "Host not found");
}

#[test]
fn test_sync_error_display_timeout() {
    let error = SynchronizationError::Timeout("30 seconds".to_string());
    assert_eq!(error.to_string(), "Timeout: 30 seconds");
}

#[test]
fn test_sync_error_display_server_error() {
    let error = SynchronizationError::ServerError(503);
    assert_eq!(error.to_string(), "503");
}

#[test]
fn test_sync_error_display_invalid_response() {
    let error = SynchronizationError::InvalidResponse("JSON parse error".to_string());
    assert_eq!(error.to_string(), "JSON parse error");
}

#[test]
fn test_sync_error_display_missing_public_key() {
    let error = SynchronizationError::MissingPublicKey("device123".to_string());
    assert_eq!(
        error.to_string(),
        "Missing public key for device: device123"
    );
}

#[test]
fn test_sync_error_display_other() {
    let error = SynchronizationError::Other("Custom error message".to_string());
    assert_eq!(error.to_string(), "Custom error message");
}

#[test]
fn test_sync_error_display_all_simple_variants() {
    let test_cases = vec![
        (
            SynchronizationError::CompressionFailed,
            "Compression failed",
        ),
        (SynchronizationError::ContentMissing, "Content is missing"),
        (
            SynchronizationError::ContentTooLarge,
            "Content is too large to share",
        ),
        (
            SynchronizationError::DeviceIdsMissing,
            "Device IDs are missing",
        ),
        (SynchronizationError::DeviceKeyMissing, "Key is missing"),
        (SynchronizationError::EmailMissing, "Email is missing"),
        (SynchronizationError::EncryptionFailed, "Encryption failed"),
        (
            SynchronizationError::FileNameMissing,
            "File name is missing",
        ),
        (
            SynchronizationError::MissingEncryptionKey,
            "Missing encryption key",
        ),
        (
            SynchronizationError::MissingExpirationDate,
            "Missing expiration date",
        ),
        (
            SynchronizationError::ServerUrlMissing,
            "Server URL is missing",
        ),
    ];

    for (error, expected_message) in test_cases {
        assert_eq!(error.to_string(), expected_message);
    }
}
