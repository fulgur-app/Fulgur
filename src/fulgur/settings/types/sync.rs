use serde::{Deserialize, Serialize};

/// Stable identifier for a server profile. Generated as a UUID v4 string
/// at profile creation and never reused.
pub type ProfileId = String;

/// Maximum number of server profiles a user can configure.
pub const MAX_PROFILES: usize = 16;

/// Default profile name assigned when a profile is created without an explicit name.
pub const DEFAULT_PROFILE_NAME: &str = "Fulgurant";

/// Generate a new unique profile id.
///
/// ### Returns
/// - `ProfileId`: A freshly generated UUID v4 string.
#[must_use]
pub fn new_profile_id() -> ProfileId {
    uuid::Uuid::new_v4().to_string()
}

/// Default name used when a profile is deserialized without a name field
/// (only happens with hand-edited config files).
///
/// ### Returns
/// - `String`: The default profile name.
fn default_profile_name() -> String {
    DEFAULT_PROFILE_NAME.to_string()
}

/// Configuration for a single Fulgurant sync server.
///
/// ### Fields
/// - `id`: Stable UUID v4 string assigned at creation.
/// - `name`: Human-readable label shown in the settings UI.
/// - `is_active`: Per-profile activation flag (independent of the master switch).
/// - `server_url`: Sync server URL, or `None` if not yet configured.
/// - `email`: Login email for the sync server, or `None`.
/// - `public_key`: X25519 public key advertised to the server. Paired with
///   the per-profile private key stored in the system keychain.
/// - `is_deduplication`: When true, the server deduplicates shares of the
///   same file path.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct ServerProfile {
    pub id: ProfileId,
    #[serde(default = "default_profile_name")]
    pub name: String,
    #[serde(default)]
    pub is_active: bool,
    pub server_url: Option<String>,
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub public_key: Option<String>,
    #[serde(default = "default_is_deduplication")]
    pub is_deduplication: bool,
}

impl ServerProfile {
    /// Create a new empty profile with a freshly generated id.
    ///
    /// ### Arguments
    /// - `name`: The display name to assign to the new profile.
    ///
    /// ### Returns
    /// - `Self`: A new profile with default values.
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            id: new_profile_id(),
            name: name.into(),
            is_active: false,
            server_url: None,
            email: None,
            public_key: None,
            is_deduplication: default_is_deduplication(),
        }
    }
}

/// Top-level synchronization configuration.
#[derive(Clone, Serialize, Deserialize, PartialEq)]
pub struct SynchronizationSettings {
    #[serde(default)]
    pub is_synchronization_activated: bool,
    #[serde(default)]
    pub profiles: Vec<ServerProfile>,
}

impl Default for SynchronizationSettings {
    fn default() -> Self {
        Self::new()
    }
}

impl SynchronizationSettings {
    /// Create a new empty synchronization settings instance.
    ///
    /// ### Returns
    /// - `SynchronizationSettings`: An empty configuration with sync disabled
    ///   and no profiles.
    #[must_use]
    pub fn new() -> Self {
        Self {
            is_synchronization_activated: false,
            profiles: Vec::new(),
        }
    }

    /// Get a reference to the first profile, if any.
    ///
    /// ### Returns
    /// - `Some(&ServerProfile)`: The first profile when at least one is configured.
    /// - `None`: When no profiles are configured.
    #[must_use]
    pub fn primary_profile(&self) -> Option<&ServerProfile> {
        self.profiles.first()
    }

    /// Get a mutable reference to the first profile, if any.
    ///
    /// ### Returns
    /// - `Some(&mut ServerProfile)`: The first profile when at least one is configured.
    /// - `None`: When no profiles are configured.
    pub fn primary_profile_mut(&mut self) -> Option<&mut ServerProfile> {
        self.profiles.first_mut()
    }

    /// Get a mutable reference to the first profile, creating one if needed.
    ///
    /// ### Returns
    /// - `&mut ServerProfile`: The first profile, creating a new "Fulgurant"
    ///   profile if the list was empty.
    pub fn ensure_primary_profile_mut(&mut self) -> &mut ServerProfile {
        if self.profiles.is_empty() {
            self.profiles.push(ServerProfile::new(DEFAULT_PROFILE_NAME));
        }
        &mut self.profiles[0]
    }

    /// Look up a profile by id.
    ///
    /// ### Arguments
    /// - `profile_id`: The id to search for.
    ///
    /// ### Returns
    /// - `Some(&ServerProfile)`: The matching profile.
    /// - `None`: When no profile with that id exists.
    #[must_use]
    pub fn find_profile(&self, profile_id: &str) -> Option<&ServerProfile> {
        self.profiles.iter().find(|p| p.id == profile_id)
    }

    /// Look up a profile by id with mutable access.
    ///
    /// ### Arguments
    /// - `profile_id`: The id to search for.
    ///
    /// ### Returns
    /// - `Some(&mut ServerProfile)`: The matching profile.
    /// - `None`: When no profile with that id exists.
    pub fn find_profile_mut(&mut self, profile_id: &str) -> Option<&mut ServerProfile> {
        self.profiles.iter_mut().find(|p| p.id == profile_id)
    }

    /// Check whether a name is already used by another profile.
    ///
    /// ### Arguments
    /// - `candidate`: The candidate display name.
    /// - `exclude_id`: Profile id to skip during the comparison.
    ///
    /// ### Returns
    /// - `true`: At least one other profile carries the same normalized name.
    /// - `false`: The name is unique among the other profiles.
    #[must_use]
    pub fn name_collides(&self, candidate: &str, exclude_id: Option<&str>) -> bool {
        let normalized = candidate.trim().to_lowercase();
        self.profiles.iter().any(|profile| {
            if exclude_id == Some(profile.id.as_str()) {
                return false;
            }
            profile.name.trim().to_lowercase() == normalized
        })
    }
}

/// Default value for `is_deduplication` setting
///
/// ### Returns
/// - `true`: enable deduplication by default
fn default_is_deduplication() -> bool {
    true
}
