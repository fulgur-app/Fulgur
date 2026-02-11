//! Integration tests for Settings save/load functionality
//!
//! These tests verify that Settings can be serialized to JSON, saved to disk,
//! and deserialized back with full fidelity. They run in CI/CD environments
//! using temporary directories for isolation.

use std::path::PathBuf;
use tempfile::TempDir;

// Import from the main crate
use fulgur::fulgur::settings::Settings;

/// Create a Settings instance with all non-default values for thorough testing
///
/// ### Returns
/// - `Settings`: A settings instance with custom values in every field
fn create_custom_settings() -> Settings {
    let mut settings = Settings::new();
    settings.editor_settings.show_line_numbers = false;
    settings.editor_settings.show_indent_guides = false;
    settings.editor_settings.soft_wrap = true;
    settings.editor_settings.font_size = 16.5;
    settings.editor_settings.tab_size = 2;
    settings.editor_settings.watch_files = false;
    settings
        .editor_settings
        .markdown_settings
        .show_markdown_preview = false;
    settings
        .editor_settings
        .markdown_settings
        .show_markdown_toolbar = true;
    settings.app_settings.confirm_exit = false;
    settings.app_settings.theme = "Tokyo Night".into();
    settings.app_settings.scrollbar_show = Some(gpui_component::scroll::ScrollbarShow::Always);
    settings
        .app_settings
        .synchronization_settings
        .is_synchronization_activated = true;
    settings.app_settings.synchronization_settings.server_url =
        Some("https://sync.example.com".to_string());
    settings.app_settings.synchronization_settings.email = Some("user@example.com".to_string());
    settings.app_settings.synchronization_settings.public_key =
        Some("test_public_key_base64".to_string());
    settings
        .app_settings
        .synchronization_settings
        .is_deduplication = false;
    settings
        .recent_files
        .add_file(PathBuf::from("/path/to/file1.txt"));
    settings
        .recent_files
        .add_file(PathBuf::from("/path/to/file2.rs"));
    settings
        .recent_files
        .add_file(PathBuf::from("/path/to/file3.md"));

    settings
}

/// Create a temporary file path for testing
///
/// ### Arguments
/// - `temp_dir`: The temporary directory
///
/// ### Returns
/// - `PathBuf`: Path to a settings.json file in the temp directory
fn temp_settings_path(temp_dir: &TempDir) -> PathBuf {
    temp_dir.path().join("settings.json")
}

/// Assert two Settings instances are equal
///
/// This helper provides detailed error messages showing which field differs
///
/// ### Arguments
/// - `original`: The original settings
/// - `loaded`: The loaded settings
/// - `context`: Description of what's being tested
fn assert_settings_equal(original: &Settings, loaded: &Settings, context: &str) {
    assert_eq!(
        original.editor_settings.show_line_numbers, loaded.editor_settings.show_line_numbers,
        "{}: show_line_numbers mismatch",
        context
    );
    assert_eq!(
        original.editor_settings.show_indent_guides, loaded.editor_settings.show_indent_guides,
        "{}: show_indent_guides mismatch",
        context
    );
    assert_eq!(
        original.editor_settings.soft_wrap, loaded.editor_settings.soft_wrap,
        "{}: soft_wrap mismatch",
        context
    );
    assert_eq!(
        original.editor_settings.font_size, loaded.editor_settings.font_size,
        "{}: font_size mismatch",
        context
    );
    assert_eq!(
        original.editor_settings.tab_size, loaded.editor_settings.tab_size,
        "{}: tab_size mismatch",
        context
    );
    assert_eq!(
        original.editor_settings.watch_files, loaded.editor_settings.watch_files,
        "{}: watch_files mismatch",
        context
    );
    assert_eq!(
        original
            .editor_settings
            .markdown_settings
            .show_markdown_preview,
        loaded
            .editor_settings
            .markdown_settings
            .show_markdown_preview,
        "{}: show_markdown_preview mismatch",
        context
    );
    assert_eq!(
        original
            .editor_settings
            .markdown_settings
            .show_markdown_toolbar,
        loaded
            .editor_settings
            .markdown_settings
            .show_markdown_toolbar,
        "{}: show_markdown_toolbar mismatch",
        context
    );
    assert_eq!(
        original.app_settings.confirm_exit, loaded.app_settings.confirm_exit,
        "{}: confirm_exit mismatch",
        context
    );
    assert_eq!(
        original.app_settings.theme, loaded.app_settings.theme,
        "{}: theme mismatch",
        context
    );
    assert_eq!(
        original.app_settings.scrollbar_show, loaded.app_settings.scrollbar_show,
        "{}: scrollbar_show mismatch",
        context
    );
    assert_eq!(
        original
            .app_settings
            .synchronization_settings
            .is_synchronization_activated,
        loaded
            .app_settings
            .synchronization_settings
            .is_synchronization_activated,
        "{}: is_synchronization_activated mismatch",
        context
    );
    assert_eq!(
        original.app_settings.synchronization_settings.server_url,
        loaded.app_settings.synchronization_settings.server_url,
        "{}: server_url mismatch",
        context
    );
    assert_eq!(
        original.app_settings.synchronization_settings.email,
        loaded.app_settings.synchronization_settings.email,
        "{}: email mismatch",
        context
    );
    assert_eq!(
        original.app_settings.synchronization_settings.public_key,
        loaded.app_settings.synchronization_settings.public_key,
        "{}: public_key mismatch",
        context
    );
    assert_eq!(
        original
            .app_settings
            .synchronization_settings
            .is_deduplication,
        loaded
            .app_settings
            .synchronization_settings
            .is_deduplication,
        "{}: is_deduplication mismatch",
        context
    );
    assert_eq!(
        original.recent_files.get_files(),
        loaded.recent_files.get_files(),
        "{}: recent_files mismatch",
        context
    );
}

// ============================================================================
// Integration Tests
// ============================================================================

#[test]
fn test_settings_roundtrip_with_default_values() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = Settings::new();
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert_settings_equal(&original, &loaded, "Default settings roundtrip");
}

#[test]
fn test_settings_roundtrip_with_custom_values() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = create_custom_settings();
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert_settings_equal(&original, &loaded, "Custom settings roundtrip");
}

#[test]
fn test_settings_optional_fields_none() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = Settings::new();
    original.app_settings.scrollbar_show = None;
    original.app_settings.synchronization_settings.server_url = None;
    original.app_settings.synchronization_settings.email = None;
    original.app_settings.synchronization_settings.public_key = None;
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert_eq!(
        loaded.app_settings.scrollbar_show, None,
        "scrollbar_show should be None"
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.server_url, None,
        "server_url should be None"
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.email, None,
        "email should be None"
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.public_key, None,
        "public_key should be None"
    );
}

#[test]
fn test_settings_optional_fields_some() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = Settings::new();
    original.app_settings.scrollbar_show = Some(gpui_component::scroll::ScrollbarShow::Hover);
    original.app_settings.synchronization_settings.server_url =
        Some("https://test.server".to_string());
    original.app_settings.synchronization_settings.email = Some("test@test.com".to_string());
    original.app_settings.synchronization_settings.public_key = Some("pubkey123".to_string());
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert_eq!(
        loaded.app_settings.scrollbar_show,
        Some(gpui_component::scroll::ScrollbarShow::Hover)
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.server_url,
        Some("https://test.server".to_string())
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.email,
        Some("test@test.com".to_string())
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.public_key,
        Some("pubkey123".to_string())
    );
}

#[test]
fn test_settings_recent_files_empty() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = Settings::new();
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert!(
        loaded.recent_files.get_files().is_empty(),
        "Recent files should be empty"
    );
}

#[test]
fn test_settings_recent_files_multiple() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = Settings::new();
    let file1 = PathBuf::from("/tmp/test1.txt");
    let file2 = PathBuf::from("/tmp/test2.rs");
    let file3 = PathBuf::from("/home/user/doc.md");
    original.recent_files.add_file(file1.clone());
    original.recent_files.add_file(file2.clone());
    original.recent_files.add_file(file3.clone());
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    let loaded_files = loaded.recent_files.get_files();
    assert_eq!(loaded_files.len(), 3, "Should have 3 recent files");
    assert_eq!(loaded_files[0], file1);
    assert_eq!(loaded_files[1], file2);
    assert_eq!(loaded_files[2], file3);
}

#[test]
fn test_settings_recent_files_respects_max_limit() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut original = Settings::new(); // max_files is 10
    for i in 0..12 {
        original
            .recent_files
            .add_file(PathBuf::from(format!("/tmp/file{}.txt", i)));
    }
    original
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert_eq!(
        loaded.recent_files.get_files().len(),
        10,
        "Should respect max_files limit"
    );
    let loaded_files = loaded.recent_files.get_files();
    assert_eq!(loaded_files[0], PathBuf::from("/tmp/file2.txt"));
    assert_eq!(loaded_files[9], PathBuf::from("/tmp/file11.txt"));
}

#[test]
fn test_settings_json_format_is_valid() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut settings = create_custom_settings();
    settings
        .save_to_path(&settings_path)
        .expect("Failed to save settings");
    let json_content =
        std::fs::read_to_string(&settings_path).expect("Failed to read settings file");
    let parsed: serde_json::Value =
        serde_json::from_str(&json_content).expect("JSON should be valid");
    assert!(parsed["editor_settings"].is_object());
    assert!(parsed["app_settings"].is_object());
    assert!(parsed["recent_files"].is_object());
    assert!(parsed["app_settings"]["synchronization_settings"].is_object());
    assert!(parsed["editor_settings"]["markdown_settings"].is_object());
}

#[test]
fn test_settings_backward_compatibility_with_missing_fields() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let minimal_json = r#"{
        "editor_settings": {
            "show_line_numbers": true,
            "show_indent_guides": true,
            "soft_wrap": false,
            "font_size": 14.0,
            "tab_size": 4,
            "markdown_settings": {
                "show_markdown_preview": true,
                "show_markdown_toolbar": false
            }
        },
        "app_settings": {
            "confirm_exit": true,
            "theme": "Default Light",
            "synchronization_settings": {
                "is_synchronization_activated": false
            }
        },
        "recent_files": {
            "files": [],
            "max_files": 10
        }
    }"#;
    std::fs::write(&settings_path, minimal_json).expect("Failed to write minimal JSON");
    let loaded = Settings::load_from_path(&settings_path).expect("Failed to load settings");
    assert_eq!(
        loaded.editor_settings.watch_files, true,
        "watch_files should default to true"
    );
    assert_eq!(
        loaded
            .app_settings
            .synchronization_settings
            .is_deduplication,
        true,
        "is_deduplication should default to true"
    );
    assert_eq!(
        loaded.app_settings.synchronization_settings.server_url, None,
        "server_url should default to None"
    );
    assert_eq!(
        loaded.app_settings.scrollbar_show, None,
        "scrollbar_show should default to None"
    );
}

#[test]
fn test_settings_multiple_save_load_cycles() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let settings_path = temp_settings_path(&temp_dir);
    let mut settings = create_custom_settings();
    for i in 0..5 {
        settings
            .save_to_path(&settings_path)
            .expect(&format!("Failed to save on iteration {}", i));
        let loaded = Settings::load_from_path(&settings_path)
            .expect(&format!("Failed to load on iteration {}", i));
        assert_settings_equal(&settings, &loaded, &format!("Iteration {} roundtrip", i));
        settings.editor_settings.font_size += 0.5;
        settings = loaded;
    }
}

#[test]
fn test_settings_load_nonexistent_file_returns_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nonexistent_path = temp_dir.path().join("does_not_exist.json");
    let result = Settings::load_from_path(&nonexistent_path);
    assert!(
        result.is_err(),
        "Loading non-existent file should return an error"
    );
}

#[test]
fn test_settings_load_invalid_json_returns_error() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let invalid_path = temp_settings_path(&temp_dir);
    std::fs::write(&invalid_path, "{ this is not valid json }")
        .expect("Failed to write invalid JSON");
    let result = Settings::load_from_path(&invalid_path);
    assert!(
        result.is_err(),
        "Loading invalid JSON should return an error"
    );
}

#[test]
fn test_settings_save_creates_parent_directory() {
    let temp_dir = TempDir::new().expect("Failed to create temp directory");
    let nested_path = temp_dir
        .path()
        .join("nested")
        .join("dir")
        .join("settings.json");
    std::fs::create_dir_all(nested_path.parent().unwrap())
        .expect("Failed to create parent directories");
    let mut settings = Settings::new();
    let result = settings.save_to_path(&nested_path);
    assert!(result.is_ok(), "Saving to nested path should succeed");
    assert!(
        nested_path.exists(),
        "Settings file should exist at nested path"
    );
}
