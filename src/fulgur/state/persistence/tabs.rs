use crate::fulgur::sync::ssh::url::RemoteSpec;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

/// Persisted SSH/SFTP tab location metadata.
///
/// This representation intentionally excludes any credential material.
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq, Eq)]
pub struct SerializedRemoteSpec {
    /// Remote hostname
    pub host: String,
    /// SSH port
    pub port: u16,
    /// Remote username. Empty means "prompt user".
    pub user: String,
    /// Remote file path
    pub path: String,
}

impl SerializedRemoteSpec {
    /// Build a persisted remote spec from a runtime `RemoteSpec`.
    ///
    /// ### Arguments
    /// - `spec`: Runtime remote spec to persist.
    ///
    /// ### Returns
    /// - `SerializedRemoteSpec`: Persistable remote spec with no password field.
    #[must_use]
    pub fn from_remote_spec(spec: &RemoteSpec) -> Self {
        Self {
            host: spec.host.clone(),
            port: spec.port,
            user: spec.user.clone().unwrap_or_default(),
            path: spec.path.clone(),
        }
    }

    /// Convert persisted remote metadata back into a runtime `RemoteSpec`.
    ///
    /// ### Returns
    /// - `RemoteSpec`: Runtime remote spec with `password_in_url` cleared.
    #[must_use]
    pub fn to_remote_spec(&self) -> RemoteSpec {
        RemoteSpec {
            host: self.host.clone(),
            port: self.port,
            user: (!self.user.trim().is_empty()).then_some(self.user.clone()),
            path: self.path.clone(),
            password_in_url: None,
        }
    }
}

/// Persisted state of a single editor tab
///
/// Tab IDs are not persisted as they are assigned at runtime based on position.
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    /// Display title shown in the tab bar (usually the filename)
    pub title: String,
    /// Path to the file on disk, if the tab has an associated file. `None` for unsaved/new tabs.
    pub file_path: Option<PathBuf>,
    /// The text content of the tab, stored for unsaved tabs or when the file may have been modified since last save
    pub content: Option<String>,
    /// ISO 8601 timestamp of when the content was last saved to disk. Used to detect if the file has been modified externally.
    pub last_saved: Option<String>,
    /// Serialized remote location metadata for SSH/SFTP tabs.
    #[serde(default)]
    pub remote: Option<SerializedRemoteSpec>,
    /// Whether the tab was in log view mode and should reopen in it.
    #[serde(default)]
    pub log_view: bool,
}

#[cfg(test)]
mod tests {
    use super::SerializedRemoteSpec;

    #[test]
    fn test_serialized_remote_spec_roundtrip_omits_password() {
        let spec = crate::fulgur::sync::ssh::url::RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/tmp/test.txt".to_string(),
            password_in_url: Some(zeroize::Zeroizing::new("secret".to_string())),
        };

        let serialized = SerializedRemoteSpec::from_remote_spec(&spec);
        assert_eq!(serialized.host, "example.com");
        assert_eq!(serialized.user, "alice");

        let json = serde_json::to_string(&serialized).expect("serialize serialized remote spec");
        assert!(
            !json.contains("password"),
            "serialized remote spec must not include a password field"
        );

        let restored = serialized.to_remote_spec();
        assert_eq!(restored.host, "example.com");
        assert_eq!(restored.port, 22);
        assert_eq!(restored.user.as_deref(), Some("alice"));
        assert_eq!(restored.path, "/tmp/test.txt");
        assert!(restored.password_in_url.is_none());
    }
}
