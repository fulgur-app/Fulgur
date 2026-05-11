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

    /// Get a one-line summary scoped to a single profile, used when aggregating the results of a multi-profile share.
    ///
    /// ### Returns
    /// - `String`: A short description of how many devices succeeded or failed
    ///   for this profile, including the failure reason on full failure.
    pub fn profile_scoped_summary(&self) -> String {
        let total = self.successes.len() + self.failures.len();
        if total == 0 {
            return "no devices selected".to_string();
        }
        if self.failures.is_empty() {
            if let Some((_, expiration)) = self.successes.first() {
                return format!("{total}/{total} succeeded until {expiration}");
            }
            return format!("{total}/{total} succeeded");
        }
        if self.successes.is_empty() {
            let reason = self
                .failures
                .first()
                .map(|(_, err)| err.to_string())
                .unwrap_or_else(|| "unknown error".to_string());
            return format!("0/{total} failed ({reason})");
        }
        let reason = self
            .failures
            .first()
            .map(|(_, err)| err.to_string())
            .unwrap_or_else(|| "unknown error".to_string());
        format!(
            "{}/{total} succeeded, {} failed ({reason})",
            self.successes.len(),
            self.failures.len()
        )
    }
}

/// Outcome of sharing to a single profile inside a multi-profile share operation.
#[derive(Debug)]
pub enum ProfileShareOutcome {
    /// The profile completed sharing (possibly with per-device failures).
    Completed(ShareResult),
    /// Sharing was aborted before any per-device call (e.g. validation failed).
    Aborted(SynchronizationError),
}

impl ProfileShareOutcome {
    /// Get a one-line summary for this profile's outcome.
    ///
    /// ### Returns
    /// - `String`: A short description suitable for inclusion in an aggregated
    ///   multi-profile summary.
    pub fn summary(&self) -> String {
        match self {
            ProfileShareOutcome::Completed(result) => result.profile_scoped_summary(),
            ProfileShareOutcome::Aborted(err) => format!("aborted ({err})"),
        }
    }

    /// Whether this profile contributed any successful share.
    ///
    /// ### Returns
    /// - `true`: At least one device on this profile received the file.
    /// - `false`: No device on this profile received the file.
    pub fn has_success(&self) -> bool {
        match self {
            ProfileShareOutcome::Completed(result) => !result.successes.is_empty(),
            ProfileShareOutcome::Aborted(_) => false,
        }
    }

    /// Whether this profile recorded any failure (per-device or upfront).
    ///
    /// ### Returns
    /// - `true`: The profile has at least one failure to report.
    /// - `false`: The profile completed without any failure.
    pub fn has_failure(&self) -> bool {
        match self {
            ProfileShareOutcome::Completed(result) => !result.failures.is_empty(),
            ProfileShareOutcome::Aborted(_) => true,
        }
    }
}

/// Format a multi-profile share summary message.
///
/// ### Arguments
/// - `outcomes`: Each entry pairs a profile display name with its outcome.
///
/// ### Returns
/// - `String`: A multi-line summary with one entry per profile.
pub fn format_multi_profile_summary(outcomes: &[(String, ProfileShareOutcome)]) -> String {
    let any_success = outcomes.iter().any(|(_, o)| o.has_success());
    let header = if any_success {
        "File shared:"
    } else {
        "Failed to share file:"
    };
    let mut lines = String::from(header);
    for (profile_name, outcome) in outcomes {
        lines.push('\n');
        lines.push_str("  ");
        lines.push_str(profile_name);
        lines.push_str(": ");
        lines.push_str(&outcome.summary());
    }
    lines
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

    #[test]
    fn test_profile_scoped_summary_all_success() {
        let result = ShareResult {
            successes: vec![
                ("d1".to_string(), "2026-01-01".to_string()),
                ("d2".to_string(), "2026-01-01".to_string()),
            ],
            failures: vec![],
        };
        let summary = result.profile_scoped_summary();
        assert!(summary.contains("2/2 succeeded"));
        assert!(summary.contains("2026-01-01"));
    }

    #[test]
    fn test_profile_scoped_summary_all_failed_includes_reason() {
        let result = ShareResult {
            successes: vec![],
            failures: vec![("d1".to_string(), SynchronizationError::AuthenticationFailed)],
        };
        let summary = result.profile_scoped_summary();
        assert!(summary.contains("0/1 failed"));
        assert!(summary.contains("Authentication failed"));
    }

    #[test]
    fn test_profile_scoped_summary_partial_includes_first_failure_reason() {
        let result = ShareResult {
            successes: vec![("d1".to_string(), "2026-01-01".to_string())],
            failures: vec![("d2".to_string(), SynchronizationError::ConnectionFailed)],
        };
        let summary = result.profile_scoped_summary();
        assert!(summary.contains("1/2 succeeded"));
        assert!(summary.contains("1 failed"));
        assert!(summary.contains("Cannot connect"));
    }

    #[test]
    fn test_format_multi_profile_summary_mixed_success_and_failure() {
        use super::{ProfileShareOutcome, format_multi_profile_summary};
        let outcomes = vec![
            (
                "Fulgurant".to_string(),
                ProfileShareOutcome::Completed(ShareResult {
                    successes: vec![("d1".to_string(), "2026-06-10".to_string())],
                    failures: vec![],
                }),
            ),
            (
                "HomeServer".to_string(),
                ProfileShareOutcome::Completed(ShareResult {
                    successes: vec![],
                    failures: vec![("d2".to_string(), SynchronizationError::AuthenticationFailed)],
                }),
            ),
        ];
        let message = format_multi_profile_summary(&outcomes);
        assert!(message.starts_with("File shared:"));
        assert!(message.contains("Fulgurant: 1/1 succeeded until 2026-06-10"));
        assert!(message.contains("HomeServer: 0/1 failed (Authentication failed)"));
    }

    #[test]
    fn test_format_multi_profile_summary_all_failed_uses_failure_header() {
        use super::{ProfileShareOutcome, format_multi_profile_summary};
        let outcomes = vec![
            (
                "ProfileA".to_string(),
                ProfileShareOutcome::Aborted(SynchronizationError::ServerUrlMissing),
            ),
            (
                "ProfileB".to_string(),
                ProfileShareOutcome::Completed(ShareResult {
                    successes: vec![],
                    failures: vec![("d1".to_string(), SynchronizationError::ConnectionFailed)],
                }),
            ),
        ];
        let message = format_multi_profile_summary(&outcomes);
        assert!(message.starts_with("Failed to share file:"));
        assert!(message.contains("ProfileA: aborted (Server URL is missing)"));
        assert!(message.contains("ProfileB: 0/1 failed"));
    }

    #[test]
    fn test_profile_share_outcome_classification_helpers() {
        use super::ProfileShareOutcome;
        let aborted = ProfileShareOutcome::Aborted(SynchronizationError::ServerUrlMissing);
        assert!(!aborted.has_success());
        assert!(aborted.has_failure());

        let full_success = ProfileShareOutcome::Completed(ShareResult {
            successes: vec![("d".to_string(), "2026-01-01".to_string())],
            failures: vec![],
        });
        assert!(full_success.has_success());
        assert!(!full_success.has_failure());

        let partial = ProfileShareOutcome::Completed(ShareResult {
            successes: vec![("d1".to_string(), "2026-01-01".to_string())],
            failures: vec![("d2".to_string(), SynchronizationError::ConnectionFailed)],
        });
        assert!(partial.has_success());
        assert!(partial.has_failure());
    }
}
