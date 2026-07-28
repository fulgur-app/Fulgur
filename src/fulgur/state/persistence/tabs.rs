use crate::fulgur::sync::ssh::url::RemoteSpec;
use ropey::Rope;
use serde::{Deserialize, Serialize};
use std::borrow::Cow;
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

/// Buffer text attached to a persisted tab.
#[derive(Debug, Clone)]
pub enum TabContent {
    /// Text already materialized as a `String`, typically loaded from a state file.
    Text(String),
    /// A cheap rope clone, flattened lazily during serialization.
    Rope(Rope),
}

impl TabContent {
    /// Whether the content holds no characters
    ///
    /// ### Returns
    /// - `bool`: `true` when the content is empty
    #[must_use]
    pub fn is_empty(&self) -> bool {
        match self {
            Self::Text(text) => text.is_empty(),
            Self::Rope(rope) => rope.len() == 0,
        }
    }

    /// Flatten the content into an owned `String`
    ///
    /// ### Returns
    /// - `String`: The full buffer text
    #[must_use]
    pub fn into_string(self) -> String {
        match self {
            Self::Text(text) => text,
            Self::Rope(rope) => rope.to_string(),
        }
    }

    /// Borrow the content as text, flattening a rope only if necessary
    ///
    /// ### Returns
    /// - `Cow<str>`: The buffer text, borrowed when it is already contiguous
    #[must_use]
    pub fn to_text(&self) -> Cow<'_, str> {
        match self {
            Self::Text(text) => Cow::Borrowed(text),
            Self::Rope(rope) => Cow::Owned(rope.to_string()),
        }
    }

    /// Fingerprint the content to detect whether it needs to be rewritten
    ///
    /// ### Returns
    /// - `(u64, usize)`: `(hash, byte_len)` for the content
    #[must_use]
    pub fn fingerprint(&self) -> (u64, usize) {
        match self {
            Self::Text(text) => {
                crate::fulgur::ui::tabs::editor_tab::content_fingerprint_from_str(text)
            }
            Self::Rope(rope) => {
                crate::fulgur::ui::tabs::editor_tab::content_fingerprint_from_rope(rope)
            }
        }
    }
}

impl From<String> for TabContent {
    fn from(text: String) -> Self {
        Self::Text(text)
    }
}

impl From<&str> for TabContent {
    fn from(text: &str) -> Self {
        Self::Text(text.to_string())
    }
}

impl From<Rope> for TabContent {
    fn from(rope: Rope) -> Self {
        Self::Rope(rope)
    }
}

impl PartialEq for TabContent {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Text(left), Self::Text(right)) => left == right,
            (Self::Rope(left), Self::Rope(right)) => left == right,
            (Self::Text(text), Self::Rope(rope)) | (Self::Rope(rope), Self::Text(text)) => {
                rope == text.as_str()
            }
        }
    }
}

impl PartialEq<str> for TabContent {
    fn eq(&self, other: &str) -> bool {
        match self {
            Self::Text(text) => text == other,
            Self::Rope(rope) => rope == other,
        }
    }
}

impl Serialize for TabContent {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        match self {
            Self::Text(text) => serializer.serialize_str(text),
            Self::Rope(rope) => serializer.collect_str(rope),
        }
    }
}

impl<'de> Deserialize<'de> for TabContent {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        String::deserialize(deserializer).map(Self::Text)
    }
}

/// Persisted state of a single editor tab
#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct TabState {
    #[serde(default)]
    pub tab_id: u64,
    /// Display title shown in the tab bar (usually the filename)
    pub title: String,
    /// Path to the file on disk, if the tab has an associated file. `None` for unsaved/new tabs.
    pub file_path: Option<PathBuf>,
    /// The text content of the tab, stored for unsaved tabs or when the file may have been modified since last save
    pub content: Option<TabContent>,
    /// ISO 8601 timestamp of when the content was last saved to disk. Used to detect if the file has been modified externally.
    pub last_saved: Option<String>,
    /// Serialized remote location metadata for SSH/SFTP tabs.
    #[serde(default)]
    pub remote: Option<SerializedRemoteSpec>,
    /// Whether the tab was in log view mode and should reopen in it.
    #[serde(default)]
    pub log_view: bool,
    /// Stable key of the tab's color tag, if any. See `ColorTag::key`.
    #[serde(default)]
    pub color_tag: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{SerializedRemoteSpec, TabContent};
    use ropey::Rope;

    /// Text exercising every JSON escape class plus multibyte and chunk-splitting input.
    fn tricky_text() -> String {
        let mut text = String::from("line1\n\t\"quoted\" \\ backslash\u{7}héllo 文档 🚀\r\n");
        text.push_str(&"padding to force several rope chunks ".repeat(200));
        text
    }

    #[test]
    fn rope_content_serializes_identically_to_text_content() {
        let text = tricky_text();
        let as_text =
            serde_json::to_string(&TabContent::Text(text.clone())).expect("serialize text content");
        let as_rope = serde_json::to_string(&TabContent::Rope(Rope::from_str(&text)))
            .expect("serialize rope content");
        assert_eq!(as_text, as_rope);
    }

    #[test]
    fn rope_content_roundtrips_through_json_as_text() {
        let text = tricky_text();
        let json = serde_json::to_string(&TabContent::Rope(Rope::from_str(&text)))
            .expect("serialize rope content");
        let restored: TabContent = serde_json::from_str(&json).expect("deserialize tab content");
        assert!(matches!(restored, TabContent::Text(_)));
        assert_eq!(restored.into_string(), text);
    }

    #[test]
    fn rope_and_text_contents_compare_equal_for_the_same_text() {
        let text = tricky_text();
        assert_eq!(
            TabContent::Text(text.clone()),
            TabContent::Rope(Rope::from_str(&text))
        );
        assert_ne!(
            TabContent::Text(text.clone()),
            TabContent::Rope(Rope::from_str("other"))
        );
        assert_eq!(TabContent::Rope(Rope::from_str(&text)), *text.as_str());
    }

    #[test]
    fn empty_content_is_reported_as_empty_for_both_variants() {
        assert!(TabContent::Text(String::new()).is_empty());
        assert!(TabContent::Rope(Rope::new()).is_empty());
        assert!(!TabContent::Rope(Rope::from_str("x")).is_empty());
    }

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
