//! Portable mode: Fulgur keeps all of its data next to the executable.
//!
//! A binary built with the `portable` cargo feature calls [`enable`] at the very
//! start of `main()`. From then on, [`crate::fulgur::utils::paths::config_dir`]
//! resolves to `<portable root>/data`, sync secrets go to a file in that folder
//! instead of the OS keychain, and file paths on the same volume as the portable
//! root are persisted relative to it, so the session survives a drive letter or
//! mount point change.
//!
//! The mode is a runtime state set by the binary only. The library never enables
//! it on its own, so tests built with `--all-features` keep the regular layout.

mod relative_paths;

pub use relative_paths::{from_persisted_path, to_persisted_path};

use anyhow::anyhow;
use std::{
    fs,
    path::{Path, PathBuf},
    sync::OnceLock,
};

/// Name of the folder, next to the portable root, that holds every Fulgur file.
pub const DATA_DIR_NAME: &str = "data";

/// Windows Application User Model ID of an installed Fulgur.
pub const INSTALLED_APP_USER_MODEL_ID: &str = "app.fulgur.fulgur";

/// Windows Application User Model ID of a portable Fulgur, kept distinct so the
/// taskbar group and the jump list never merge with an installed Fulgur.
pub const PORTABLE_APP_USER_MODEL_ID: &str = "app.fulgur.fulgur.portable";

/// Linux window application id of an installed Fulgur (Wayland `app_id`, X11
/// `WM_CLASS`). Must match `StartupWMClass` in `resources/linux/Fulgur.desktop.hbs`.
pub const INSTALLED_WINDOW_APP_ID: &str = "Fulgur";

/// Linux window application id of a portable Fulgur, kept distinct so its windows
/// never group with an installed Fulgur's or start it from a dock launcher. The
/// release workflow writes it to `StartupWMClass` in the portable `AppImage`.
pub const PORTABLE_WINDOW_APP_ID: &str = "FulgurPortable";

/// Name of the probe file written to check that the data folder is writable.
const WRITE_PROBE_FILE_NAME: &str = ".fulgur-write-probe";

static LAYOUT: OnceLock<PortableLayout> = OnceLock::new();

/// Where a portable Fulgur lives and which volume holds it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PortableLayout {
    /// Folder holding the executable (or the `AppImage` on Linux).
    root: PathBuf,
    /// Root of the volume holding `root`: the drive (`E:\`) on Windows, the
    /// mount point on Unix. Paths under it are persisted relative to `root`.
    volume_root: PathBuf,
}

impl PortableLayout {
    /// Build a layout from an explicit root and volume root.
    ///
    /// ### Arguments
    /// - `root`: The folder holding the executable
    /// - `volume_root`: The root of the volume holding `root`
    ///
    /// ### Returns
    /// - `PortableLayout`: The layout
    #[must_use]
    pub fn new(root: PathBuf, volume_root: PathBuf) -> Self {
        Self { root, volume_root }
    }

    /// Build the layout of a portable root, detecting the volume that holds it.
    ///
    /// ### Arguments
    /// - `root`: The folder holding the executable
    ///
    /// ### Returns
    /// - `PortableLayout`: The layout with its detected volume root
    #[must_use]
    pub fn detect(root: PathBuf) -> Self {
        let volume_root = volume_root_of(&root);
        Self { root, volume_root }
    }

    /// The folder holding the executable.
    ///
    /// ### Returns
    /// - `&Path`: The portable root
    #[must_use]
    pub fn root(&self) -> &Path {
        &self.root
    }

    /// The root of the volume holding the portable root.
    ///
    /// ### Returns
    /// - `&Path`: The drive on Windows, the mount point on Unix
    #[must_use]
    pub fn volume_root(&self) -> &Path {
        &self.volume_root
    }

    /// The folder holding every Fulgur file.
    ///
    /// ### Returns
    /// - `PathBuf`: `<root>/data`
    #[must_use]
    pub fn data_dir(&self) -> PathBuf {
        self.root.join(DATA_DIR_NAME)
    }
}

/// Switch the process to portable mode.
///
/// ### Errors
/// - Returns an error if the executable path cannot be resolved, or if the data
///   folder cannot be created or written to.
///
/// ### Returns
/// - `Ok(&PortableLayout)`: The active portable layout
/// - `Err(anyhow::Error)`: Fulgur cannot run portable from this location
pub fn enable() -> anyhow::Result<&'static PortableLayout> {
    if let Some(layout) = LAYOUT.get() {
        return Ok(layout);
    }
    let exe =
        std::env::current_exe().map_err(|e| anyhow!("Cannot locate the Fulgur executable: {e}"))?;
    let root = portable_root_from(&exe, appimage_path().as_deref()).ok_or_else(|| {
        anyhow!(
            "Cannot determine the folder holding the Fulgur executable '{}'",
            exe.display()
        )
    })?;
    let layout = PortableLayout::detect(root);
    ensure_writable_dir(&layout.data_dir())?;
    Ok(LAYOUT.get_or_init(|| layout))
}

/// The active portable layout.
///
/// ### Returns
/// - `Some(&PortableLayout)`: The process runs in portable mode
/// - `None`: The process uses the regular user profile layout
#[must_use]
pub fn layout() -> Option<&'static PortableLayout> {
    LAYOUT.get()
}

/// Whether the process runs in portable mode.
///
/// ### Returns
/// - `true`: Data lives next to the executable
/// - `false`: Data lives in the user profile
#[must_use]
pub fn is_portable() -> bool {
    LAYOUT.get().is_some()
}

/// The Windows Application User Model ID of this process.
///
/// ### Returns
/// - `&'static str`: The portable or the installed AUMID
#[must_use]
pub fn app_user_model_id() -> &'static str {
    app_user_model_id_for(is_portable())
}

/// The Windows Application User Model ID for a portable or an installed Fulgur.
///
/// ### Arguments
/// - `portable`: Whether the process runs in portable mode
///
/// ### Returns
/// - `&'static str`: The matching AUMID
#[must_use]
pub fn app_user_model_id_for(portable: bool) -> &'static str {
    if portable {
        PORTABLE_APP_USER_MODEL_ID
    } else {
        INSTALLED_APP_USER_MODEL_ID
    }
}

/// The Linux window application id of this process.
///
/// ### Returns
/// - `&'static str`: The portable or the installed window application id
#[must_use]
pub fn window_app_id() -> &'static str {
    window_app_id_for(is_portable())
}

/// The Linux window application id for a portable or an installed Fulgur.
///
/// ### Arguments
/// - `portable`: Whether the process runs in portable mode
///
/// ### Returns
/// - `&'static str`: The matching window application id
#[must_use]
pub fn window_app_id_for(portable: bool) -> &'static str {
    if portable {
        PORTABLE_WINDOW_APP_ID
    } else {
        INSTALLED_WINDOW_APP_ID
    }
}

/// The `AppImage` file the process was started from, on Linux.
///
/// ### Returns
/// - `Some(PathBuf)`: The absolute path of the running `AppImage`
/// - `None`: Not running from an `AppImage`, or not on Linux
fn appimage_path() -> Option<PathBuf> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    std::env::var_os("APPIMAGE")
        .map(PathBuf::from)
        .filter(|path| path.is_absolute())
}

/// Resolve the portable root from the executable path.
///
/// ### Arguments
/// - `exe`: The running executable
/// - `appimage`: The running `AppImage`, whose own executable sits in a read-only
///   mount, so its folder is used instead
///
/// ### Returns
/// - `Some(PathBuf)`: The folder holding the `AppImage`, else the executable
/// - `None`: The path has no parent folder
fn portable_root_from(exe: &Path, appimage: Option<&Path>) -> Option<PathBuf> {
    appimage
        .unwrap_or(exe)
        .parent()
        .filter(|parent| !parent.as_os_str().is_empty())
        .map(Path::to_path_buf)
}

/// Create a folder if needed and check that files can be written in it.
///
/// ### Arguments
/// - `dir`: The folder to check
///
/// ### Errors
/// - Returns an error, worded for the user, if the folder cannot be created or
///   if a file cannot be written in it.
///
/// ### Returns
/// - `Ok(())`: The folder exists and is writable
/// - `Err(anyhow::Error)`: The folder is missing or read-only
pub(crate) fn ensure_writable_dir(dir: &Path) -> anyhow::Result<()> {
    let unwritable = |e: std::io::Error| {
        anyhow!(
            "Fulgur Portable cannot write to its data folder '{}': {e}. \
             Move the Fulgur folder to a writable location (not a read-only drive \
             or Program Files).",
            dir.display()
        )
    };
    fs::create_dir_all(dir).map_err(unwritable)?;
    let probe = dir.join(WRITE_PROBE_FILE_NAME);
    fs::write(&probe, b"").map_err(unwritable)?;
    if let Err(e) = fs::remove_file(&probe) {
        log::warn!("Failed to remove '{}': {e}", probe.display());
    }
    Ok(())
}

/// The drive holding a path, on Windows.
///
/// ### Arguments
/// - `path`: An absolute path
///
/// ### Returns
/// - `PathBuf`: The prefix and root of the path (`E:\` or `\\server\share\`)
#[cfg(windows)]
fn volume_root_of(path: &Path) -> PathBuf {
    use std::path::Component;
    path.components()
        .take_while(|component| matches!(component, Component::Prefix(_) | Component::RootDir))
        .collect()
}

/// The mount point holding a path, on Unix.
///
/// ### Arguments
/// - `path`: An absolute path to an existing folder
///
/// ### Returns
/// - `PathBuf`: The mount point of the file system holding `path`
#[cfg(unix)]
fn volume_root_of(path: &Path) -> PathBuf {
    use std::os::unix::fs::MetadataExt;
    let Ok(device) = fs::metadata(path).map(|metadata| metadata.dev()) else {
        return path.to_path_buf();
    };
    let mut mount_point = path;
    for ancestor in path.ancestors().skip(1) {
        match fs::metadata(ancestor) {
            Ok(metadata) if metadata.dev() == device => mount_point = ancestor,
            _ => break,
        }
    }
    mount_point.to_path_buf()
}

#[cfg(test)]
mod tests {
    use super::{
        INSTALLED_APP_USER_MODEL_ID, INSTALLED_WINDOW_APP_ID, PORTABLE_APP_USER_MODEL_ID,
        PORTABLE_WINDOW_APP_ID, PortableLayout, app_user_model_id_for, ensure_writable_dir,
        portable_root_from, volume_root_of, window_app_id_for,
    };
    use std::path::{Path, PathBuf};

    #[test]
    fn test_portable_root_is_the_executable_folder() {
        let dir = tempfile::tempdir().unwrap();
        let exe = dir.path().join("Fulgur.exe");

        assert_eq!(
            portable_root_from(&exe, None),
            Some(dir.path().to_path_buf())
        );
    }

    #[test]
    fn test_portable_root_is_the_appimage_folder_when_running_from_an_appimage() {
        let stick = tempfile::tempdir().unwrap();
        let mount = tempfile::tempdir().unwrap();
        let exe = mount.path().join("usr").join("bin").join("Fulgur");
        let appimage = stick.path().join("Fulgur.AppImage");

        assert_eq!(
            portable_root_from(&exe, Some(&appimage)),
            Some(stick.path().to_path_buf())
        );
    }

    #[test]
    fn test_portable_root_needs_a_parent_folder() {
        assert_eq!(portable_root_from(Path::new("Fulgur"), None), None);
    }

    #[test]
    fn test_data_dir_is_next_to_the_executable() {
        let root = PathBuf::from("stick").join("Fulgur");
        let layout = PortableLayout::new(root.clone(), PathBuf::from("stick"));

        assert_eq!(layout.data_dir(), root.join("data"));
    }

    #[test]
    fn test_portable_and_installed_builds_use_distinct_app_user_model_ids() {
        assert_eq!(app_user_model_id_for(false), INSTALLED_APP_USER_MODEL_ID);
        assert_eq!(app_user_model_id_for(true), PORTABLE_APP_USER_MODEL_ID);
        assert_ne!(app_user_model_id_for(true), app_user_model_id_for(false));
    }

    #[test]
    fn test_portable_and_installed_builds_use_distinct_window_app_ids() {
        assert_eq!(window_app_id_for(false), INSTALLED_WINDOW_APP_ID);
        assert_eq!(window_app_id_for(true), PORTABLE_WINDOW_APP_ID);
        assert_ne!(window_app_id_for(true), window_app_id_for(false));
    }

    #[test]
    fn test_installed_window_app_id_matches_the_desktop_entry() {
        let template = include_str!("../../../../resources/linux/Fulgur.desktop.hbs");

        assert!(
            template
                .lines()
                .any(|line| line == format!("StartupWMClass={INSTALLED_WINDOW_APP_ID}")),
            "the desktop entry must name the installed window app id, the release \
             workflow rewrites that exact line for the portable AppImage"
        );
    }

    #[test]
    fn test_ensure_writable_dir_creates_the_folder_and_leaves_no_probe() {
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");

        ensure_writable_dir(&data).expect("a temp folder must be writable");

        assert!(data.is_dir());
        assert_eq!(std::fs::read_dir(&data).unwrap().count(), 0);
    }

    #[test]
    fn test_ensure_writable_dir_fails_when_a_file_blocks_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let blocker = dir.path().join("data");
        std::fs::write(&blocker, "not a folder").unwrap();

        let error = ensure_writable_dir(&blocker).expect_err("a file is not a folder");

        assert!(
            error
                .to_string()
                .contains("cannot write to its data folder")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_ensure_writable_dir_fails_on_a_read_only_folder() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let data = dir.path().join("data");
        std::fs::create_dir(&data).unwrap();
        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o555)).unwrap();
        let probe_succeeded = std::fs::write(data.join("root-check"), b"").is_ok();

        let result = ensure_writable_dir(&data);

        std::fs::set_permissions(&data, std::fs::Permissions::from_mode(0o755)).unwrap();
        if probe_succeeded {
            // Running as root: permissions are not enforced, nothing to assert.
            return;
        }
        assert!(result.is_err(), "a read-only folder must be rejected");
    }

    #[cfg(unix)]
    #[test]
    fn test_volume_root_is_an_ancestor_on_the_same_device() {
        use std::os::unix::fs::MetadataExt;
        let dir = tempfile::tempdir().unwrap();

        let volume_root = volume_root_of(dir.path());

        assert!(dir.path().starts_with(&volume_root));
        assert_eq!(
            std::fs::metadata(&volume_root).unwrap().dev(),
            std::fs::metadata(dir.path()).unwrap().dev()
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_volume_root_is_the_drive() {
        assert_eq!(
            volume_root_of(Path::new(r"E:\Tools\Fulgur")),
            PathBuf::from(r"E:\")
        );
    }
}
