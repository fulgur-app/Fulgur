use crate::fulgur::sync::ssh::url::RemoteSpec;
use std::path::PathBuf;

/// The source location of an editor tab's content.
#[derive(Debug, Clone)]
pub enum TabLocation {
    /// A file on the local filesystem.
    Local(PathBuf),
    /// A file on a remote SSH/SFTP host.
    Remote(RemoteSpec),
    /// An unsaved buffer with no associated file.
    Untitled,
}

impl TabLocation {
    /// Return the local filesystem path if this is a local file.
    ///
    /// ### Returns
    /// - `Some(&PathBuf)`: The local path.
    /// - `None`: If the location is remote or untitled.
    pub fn local_path(&self) -> Option<&PathBuf> {
        match self {
            TabLocation::Local(path) => Some(path),
            _ => None,
        }
    }

    /// Return a human-readable path string for UI display.
    ///
    /// ### Returns
    /// - `String`: The file path for local tabs, `user@host:path` for remote tabs, or an empty string for untitled tabs.
    pub fn display_path(&self) -> String {
        match self {
            TabLocation::Local(path) => path.to_string_lossy().into_owned(),
            TabLocation::Remote(spec) => {
                let user = spec.user.as_deref().unwrap_or("?");
                format!("{}@{}:{}", user, spec.host, spec.path)
            }
            TabLocation::Untitled => String::new(),
        }
    }

    /// Return whether this location has no associated file.
    ///
    /// ### Returns
    /// - `bool`: `true` if untitled, `false` otherwise.
    pub fn is_untitled(&self) -> bool {
        matches!(self, TabLocation::Untitled)
    }
}
