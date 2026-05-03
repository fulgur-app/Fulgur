use crate::fulgur::sync::synchronization::SynchronizationError;
use std::{path::PathBuf, sync::Arc};

/// Parameters for sharing a file
pub struct ShareFileRequest {
    /// Reference-counted snapshot of the file content.
    pub content: Arc<str>,
    pub file_name: String,
    pub device_ids: Vec<String>,
    pub file_path: Option<PathBuf>,
}

/// Result of sharing a file with devices
#[derive(Debug)]
pub struct ShareResult {
    pub successes: Vec<(String, String)>, // (device_id, expiration_date)
    pub failures: Vec<(String, SynchronizationError)>, // (device_id, error)
}

impl ShareResult {
    /// Check if all shares were successful
    ///
    /// ### Returns
    /// - `true`: If all shares were successful, `false` otherwise
    pub fn is_complete_success(&self) -> bool {
        self.failures.is_empty()
    }

    /// Get a summary message for the share operation
    ///
    /// ### Returns
    /// - `String`: The message
    pub fn summary_message(&self) -> String {
        let total = self.successes.len() + self.failures.len();
        if self.is_complete_success() {
            if let Some((_, expiration)) = self.successes.first() {
                format!("File shared successfully to {total} device(s) until {expiration}.")
            } else if total == 0 {
                "The file was not shared.".to_string()
            } else {
                "File shared successfully.".to_string()
            }
        } else if self.successes.is_empty() {
            format!("Failed to share file to all {total} device(s).")
        } else {
            format!(
                "File shared to {}/{} device(s). {} failed.",
                self.successes.len(),
                total,
                self.failures.len()
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::ShareResult;
    use crate::fulgur::sync::synchronization::SynchronizationError;

    #[test]
    fn test_share_result_is_complete_success_all_successful() {
        let result = ShareResult {
            successes: vec![
                ("device1".to_string(), "2025-01-01".to_string()),
                ("device2".to_string(), "2025-01-01".to_string()),
            ],
            failures: vec![],
        };
        assert!(result.is_complete_success());
    }

    #[test]
    fn test_share_result_is_complete_success_with_failures() {
        let result = ShareResult {
            successes: vec![("device1".to_string(), "2025-01-01".to_string())],
            failures: vec![(
                "device2".to_string(),
                SynchronizationError::ConnectionFailed,
            )],
        };
        assert!(!result.is_complete_success());
    }

    #[test]
    fn test_share_result_is_complete_success_all_failed() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![
                (
                    "device1".to_string(),
                    SynchronizationError::ConnectionFailed,
                ),
                (
                    "device2".to_string(),
                    SynchronizationError::AuthenticationFailed,
                ),
            ],
        };
        assert!(!result.is_complete_success());
    }

    #[test]
    fn test_share_result_summary_message_complete_success() {
        let result = ShareResult {
            successes: vec![
                ("device1".to_string(), "2025-12-31".to_string()),
                ("device2".to_string(), "2025-12-31".to_string()),
            ],
            failures: vec![],
        };
        let message = result.summary_message();
        assert!(message.contains("File shared successfully"));
        assert!(message.contains("2 device(s)"));
        assert!(message.contains("2025-12-31"));
    }

    #[test]
    fn test_share_result_summary_message_all_failed() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![
                (
                    "device1".to_string(),
                    SynchronizationError::ConnectionFailed,
                ),
                (
                    "device2".to_string(),
                    SynchronizationError::AuthenticationFailed,
                ),
            ],
        };
        let message = result.summary_message();
        assert!(message.contains("Failed to share file to all"));
        assert!(message.contains("2 device(s)"));
    }

    #[test]
    fn test_share_result_summary_message_partial_success() {
        let result = ShareResult {
            successes: vec![
                ("device1".to_string(), "2025-12-31".to_string()),
                ("device2".to_string(), "2025-12-31".to_string()),
            ],
            failures: vec![(
                "device3".to_string(),
                SynchronizationError::ConnectionFailed,
            )],
        };
        let message = result.summary_message();
        assert!(message.contains("2/3 device(s)"));
        assert!(message.contains("1 failed"));
    }

    #[test]
    fn test_share_result_summary_message_empty() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![],
        };
        let message = result.summary_message();
        assert_eq!(message, "The file was not shared.");
    }

    #[test]
    fn test_share_result_summary_message_single_success() {
        let result = ShareResult {
            successes: vec![("device1".to_string(), "2025-06-30".to_string())],
            failures: vec![],
        };
        let message = result.summary_message();
        assert!(message.contains("File shared successfully"));
        assert!(message.contains("1 device(s)"));
        assert!(message.contains("2025-06-30"));
    }
}
