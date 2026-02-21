//! Unit tests for SSE event parsing
//!
//! These tests verify the `SseEvent::parse()` function correctly handles
//! various event types, valid/invalid JSON, and edge cases.

use fulgur::fulgur::sync::sse::SseEvent;

// Helper functions

/// Check if an SseEvent is a Heartbeat with the expected timestamp
fn assert_heartbeat(event: &SseEvent, expected_timestamp: &str) {
    match event {
        SseEvent::Heartbeat { timestamp } => {
            assert_eq!(
                timestamp, expected_timestamp,
                "Heartbeat timestamp mismatch"
            );
        }
        _ => panic!("Expected Heartbeat event, got: {:?}", event),
    }
}

/// Check if an SseEvent is a ShareAvailable with expected file_name
fn assert_share_available(event: &SseEvent, expected_file_name: &str) {
    match event {
        SseEvent::ShareAvailable(notification) => {
            assert_eq!(
                notification.file_name, expected_file_name,
                "Share file_name mismatch"
            );
        }
        _ => panic!("Expected ShareAvailable event, got: {:?}", event),
    }
}

/// Check if an SseEvent is an Error
fn assert_error(event: &SseEvent) {
    match event {
        SseEvent::Error(_) => {} // Success
        _ => panic!("Expected Error event, got: {:?}", event),
    }
}

// Heartbeat event tests

#[test]
fn test_parse_valid_heartbeat() {
    let data = r#"{"timestamp":"2024-01-15T12:00:00Z"}"#;
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "2024-01-15T12:00:00Z");
}

#[test]
fn test_parse_heartbeat_with_milliseconds() {
    let data = r#"{"timestamp":"2024-01-15T12:00:00.123Z"}"#;
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "2024-01-15T12:00:00.123Z");
}

#[test]
fn test_parse_heartbeat_with_timezone_offset() {
    let data = r#"{"timestamp":"2024-01-15T12:00:00+05:30"}"#;
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "2024-01-15T12:00:00+05:30");
}

#[test]
fn test_parse_heartbeat_invalid_json() {
    // Invalid JSON should return Heartbeat with empty timestamp (fallback behavior)
    let data = r#"{"timestamp":invalid}"#;
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "");
}

#[test]
fn test_parse_heartbeat_missing_timestamp() {
    // Missing timestamp field should fail to deserialize, return empty timestamp
    let data = r#"{"other_field":"value"}"#;
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "");
}

#[test]
fn test_parse_heartbeat_empty_data() {
    let data = "";
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "");
}

#[test]
fn test_parse_heartbeat_whitespace_only() {
    let data = "   ";
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "");
}

// Share notification tests

#[test]
fn test_parse_valid_share_notification() {
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src",
        "destination_device_id": "device-dest",
        "file_name": "test.txt",
        "file_size": 1024,
        "file_hash": "abc123hash",
        "content": "base64encodedcontent",
        "created_at": "2024-01-15T12:00:00Z",
        "expires_at": "2024-01-16T12:00:00Z"
    }"#;
    let event = SseEvent::parse("share_available", data);
    assert_share_available(&event, "test.txt");

    // Verify all fields are parsed correctly
    match event {
        SseEvent::ShareAvailable(notification) => {
            assert_eq!(notification.share_id, "share-123");
            assert_eq!(notification.source_device_id, "device-src");
            assert_eq!(notification.destination_device_id, "device-dest");
            assert_eq!(notification.file_size, 1024);
            assert_eq!(notification.file_hash, "abc123hash");
            assert_eq!(notification.content, "base64encodedcontent");
            assert_eq!(notification.created_at, "2024-01-15T12:00:00Z");
            assert_eq!(notification.expires_at, "2024-01-16T12:00:00Z");
        }
        _ => panic!("Expected ShareAvailable event"),
    }
}

#[test]
fn test_parse_share_notification_with_unicode_filename() {
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src",
        "destination_device_id": "device-dest",
        "file_name": "文件.txt",
        "file_size": 1024,
        "file_hash": "abc123hash",
        "content": "base64encodedcontent",
        "created_at": "2024-01-15T12:00:00Z",
        "expires_at": "2024-01-16T12:00:00Z"
    }"#;
    let event = SseEvent::parse("share_available", data);
    assert_share_available(&event, "文件.txt");
}

#[test]
fn test_parse_share_notification_with_special_chars_filename() {
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src",
        "destination_device_id": "device-dest",
        "file_name": "my file (1) [copy].txt",
        "file_size": 1024,
        "file_hash": "abc123hash",
        "content": "base64encodedcontent",
        "created_at": "2024-01-15T12:00:00Z",
        "expires_at": "2024-01-16T12:00:00Z"
    }"#;
    let event = SseEvent::parse("share_available", data);
    assert_share_available(&event, "my file (1) [copy].txt");
}

#[test]
fn test_parse_share_notification_large_file_size() {
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src",
        "destination_device_id": "device-dest",
        "file_name": "large.zip",
        "file_size": 1073741824,
        "file_hash": "abc123hash",
        "content": "base64encodedcontent",
        "created_at": "2024-01-15T12:00:00Z",
        "expires_at": "2024-01-16T12:00:00Z"
    }"#;
    let event = SseEvent::parse("share_available", data);
    match event {
        SseEvent::ShareAvailable(notification) => {
            assert_eq!(notification.file_size, 1073741824); // 1GB
        }
        _ => panic!("Expected ShareAvailable event"),
    }
}

#[test]
fn test_parse_share_notification_invalid_json() {
    // Invalid JSON should return Error
    let data = r#"{"file_name":invalid}"#;
    let event = SseEvent::parse("share_available", data);
    assert_error(&event);
}

#[test]
fn test_parse_share_notification_missing_required_field() {
    // Missing required field should fail deserialization
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src"
    }"#;
    let event = SseEvent::parse("share_available", data);
    assert_error(&event);
}

#[test]
fn test_parse_share_notification_empty_data() {
    let data = "";
    let event = SseEvent::parse("share_available", data);
    assert_error(&event);
}

#[test]
fn test_parse_share_notification_wrong_type() {
    // Valid JSON but wrong structure
    let data = r#"{"timestamp":"2024-01-15T12:00:00Z"}"#;
    let event = SseEvent::parse("share_available", data);
    assert_error(&event);
}

// Unknown/invalid event types

#[test]
fn test_parse_empty_event_type() {
    let data = r#"{"some":"data"}"#;
    let event = SseEvent::parse("", data);
    assert_error(&event);
}

#[test]
fn test_parse_unknown_event_type() {
    let data = r#"{"some":"data"}"#;
    let event = SseEvent::parse("unknown_event", data);
    assert_error(&event);
}

#[test]
fn test_parse_unknown_event_type_custom() {
    let data = r#"{"message":"hello"}"#;
    let event = SseEvent::parse("custom_event", data);
    assert_error(&event);
}

// Edge cases

#[test]
fn test_parse_very_long_data() {
    // Test with a very long JSON string
    let long_content = "a".repeat(10000);
    let data = format!(
        r#"{{"timestamp":"2024-01-15T12:00:00Z","long_field":"{}"}}"#,
        long_content
    );
    let event = SseEvent::parse("heartbeat", &data);
    // Should parse successfully even with extra fields
    assert_heartbeat(&event, "2024-01-15T12:00:00Z");
}

#[test]
fn test_parse_nested_json() {
    // Heartbeat with nested structure (extra fields should be ignored)
    let data = r#"{
        "timestamp": "2024-01-15T12:00:00Z",
        "metadata": {
            "nested": "value"
        }
    }"#;
    let event = SseEvent::parse("heartbeat", data);
    assert_heartbeat(&event, "2024-01-15T12:00:00Z");
}

#[test]
fn test_parse_event_type_case_sensitivity() {
    // Event types should be case-sensitive
    let data = r#"{"timestamp":"2024-01-15T12:00:00Z"}"#;
    let event = SseEvent::parse("HEARTBEAT", data);
    assert_error(&event); // Should be unknown, not heartbeat
}

#[test]
fn test_parse_event_type_with_whitespace() {
    // Event types with whitespace should be treated as unknown
    let data = r#"{"timestamp":"2024-01-15T12:00:00Z"}"#;
    let event = SseEvent::parse(" heartbeat ", data);
    assert_error(&event);
}

#[test]
fn test_parse_share_with_zero_file_size() {
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src",
        "destination_device_id": "device-dest",
        "file_name": "empty.txt",
        "file_size": 0,
        "file_hash": "abc123hash",
        "content": "",
        "created_at": "2024-01-15T12:00:00Z",
        "expires_at": "2024-01-16T12:00:00Z"
    }"#;
    let event = SseEvent::parse("share_available", data);
    match event {
        SseEvent::ShareAvailable(notification) => {
            assert_eq!(notification.file_size, 0);
            assert_eq!(notification.content, "");
        }
        _ => panic!("Expected ShareAvailable event"),
    }
}

#[test]
fn test_parse_share_with_negative_file_size() {
    // Negative file size should be accepted by i64 type
    let data = r#"{
        "share_id": "share-123",
        "source_device_id": "device-src",
        "destination_device_id": "device-dest",
        "file_name": "test.txt",
        "file_size": -1,
        "file_hash": "abc123hash",
        "content": "base64encodedcontent",
        "created_at": "2024-01-15T12:00:00Z",
        "expires_at": "2024-01-16T12:00:00Z"
    }"#;
    let event = SseEvent::parse("share_available", data);
    match event {
        SseEvent::ShareAvailable(notification) => {
            assert_eq!(notification.file_size, -1);
        }
        _ => panic!("Expected ShareAvailable event"),
    }
}
