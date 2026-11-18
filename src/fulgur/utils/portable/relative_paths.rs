//! Conversion between in-memory paths and the paths a portable Fulgur persists.
//!
//! A path on the same volume as the portable root is persisted relative to that
//! root (`..\Docs\notes.md`), any other path stays absolute. Persisted paths were
//! always absolute before portable mode, so a relative one is unambiguous.
//!
//! The conversion is purely lexical: it never touches the file system, so the
//! same in-memory path always yields the same persisted path, which keeps the
//! change-gated row comparison of the state database stable.

use super::PortableLayout;
use std::path::{Component, Path, PathBuf};

/// Convert an in-memory path into the path to persist.
///
/// ### Arguments
/// - `path`: The absolute in-memory path
/// - `layout`: The portable layout, `None` outside portable mode
///
/// ### Returns
/// - `PathBuf`: The path relative to the portable root when it is on the same
///   volume, else the path unchanged
#[must_use]
pub fn to_persisted_path(path: &Path, layout: Option<&PortableLayout>) -> PathBuf {
    layout
        .and_then(|layout| relativize(path, layout))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Convert a persisted path back into an in-memory path.
///
/// ### Arguments
/// - `path`: The persisted path, relative or absolute
/// - `layout`: The portable layout, `None` outside portable mode
///
/// ### Returns
/// - `PathBuf`: The relative path resolved against the portable root, or the
///   path unchanged when it is absolute or outside portable mode
#[must_use]
pub fn from_persisted_path(path: &Path, layout: Option<&PortableLayout>) -> PathBuf {
    layout
        .and_then(|layout| resolve(path, layout))
        .unwrap_or_else(|| path.to_path_buf())
}

/// Express an absolute path relative to the portable root.
///
/// ### Arguments
/// - `path`: The absolute path
/// - `layout`: The portable layout
///
/// ### Returns
/// - `Some(PathBuf)`: The relative path, `path` being on the portable volume
/// - `None`: `path` is not absolute or lives on another volume
fn relativize(path: &Path, layout: &PortableLayout) -> Option<PathBuf> {
    if !path.is_absolute() || !starts_with_components(path, layout.volume_root()) {
        return None;
    }
    let target: Vec<Component<'_>> = path.components().collect();
    let root: Vec<Component<'_>> = layout.root().components().collect();
    let shared = target
        .iter()
        .zip(&root)
        .take_while(|(a, b)| components_equal(a, b))
        .count();
    let mut relative: PathBuf = std::iter::repeat_n(Component::ParentDir, root.len() - shared)
        .chain(target[shared..].iter().copied())
        .collect();
    if relative.as_os_str().is_empty() {
        relative.push(Component::CurDir);
    }
    Some(relative)
}

/// Resolve a relative persisted path against the portable root.
///
/// ### Arguments
/// - `path`: The persisted path
/// - `layout`: The portable layout
///
/// ### Returns
/// - `Some(PathBuf)`: The absolute path
/// - `None`: `path` is absolute or carries a drive or root, so it is kept as is
fn resolve(path: &Path, layout: &PortableLayout) -> Option<PathBuf> {
    if path.is_absolute() {
        return None;
    }
    let mut resolved = layout.root().to_path_buf();
    for component in path.components() {
        match component {
            Component::ParentDir => {
                resolved.pop();
            }
            Component::CurDir => {}
            Component::Normal(name) => resolved.push(name),
            Component::Prefix(_) | Component::RootDir => return None,
        }
    }
    Some(resolved)
}

/// Whether the leading components of `path` are exactly those of `prefix`.
///
/// ### Arguments
/// - `path`: The path to check
/// - `prefix`: The expected leading components
///
/// ### Returns
/// - `true`: `path` lives under `prefix`, drive letters compared case-insensitively
/// - `false`: Otherwise
fn starts_with_components(path: &Path, prefix: &Path) -> bool {
    let mut components = path.components();
    prefix.components().all(|expected| {
        components
            .next()
            .is_some_and(|actual| components_equal(&actual, &expected))
    })
}

/// Compare two path components, ignoring the case of Windows drive prefixes.
///
/// ### Arguments
/// - `a`: The first component
/// - `b`: The second component
///
/// ### Returns
/// - `true`: The components designate the same thing
/// - `false`: Otherwise
fn components_equal(a: &Component<'_>, b: &Component<'_>) -> bool {
    match (a, b) {
        (Component::Prefix(a), Component::Prefix(b)) => {
            a.as_os_str().eq_ignore_ascii_case(b.as_os_str())
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::{from_persisted_path, to_persisted_path};
    use crate::fulgur::utils::portable::PortableLayout;
    use std::path::{Path, PathBuf};

    /// Build a layout from string paths.
    ///
    /// ### Arguments
    /// - `root`: The portable root
    /// - `volume_root`: The root of its volume
    ///
    /// ### Returns
    /// - `PortableLayout`: The layout
    fn layout(root: &str, volume_root: &str) -> PortableLayout {
        PortableLayout::new(PathBuf::from(root), PathBuf::from(volume_root))
    }

    #[test]
    fn test_paths_are_unchanged_outside_portable_mode() {
        let path = std::env::temp_dir().join("notes.md");

        assert_eq!(to_persisted_path(&path, None), path);
        assert_eq!(
            from_persisted_path(Path::new("../notes.md"), None),
            PathBuf::from("../notes.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_same_volume_path_becomes_relative() {
        let stick = layout("/media/user/STICK/Fulgur", "/media/user/STICK");

        assert_eq!(
            to_persisted_path(Path::new("/media/user/STICK/Docs/notes.md"), Some(&stick)),
            PathBuf::from("../Docs/notes.md")
        );
        assert_eq!(
            to_persisted_path(Path::new("/media/user/STICK/Fulgur/todo.txt"), Some(&stick)),
            PathBuf::from("todo.txt")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_other_volume_path_stays_absolute() {
        let stick = layout("/media/user/STICK/Fulgur", "/media/user/STICK");
        let home_file = Path::new("/home/user/notes.md");
        let sibling_stick = Path::new("/media/user/STICK2/notes.md");

        assert_eq!(to_persisted_path(home_file, Some(&stick)), home_file);
        assert_eq!(
            to_persisted_path(sibling_stick, Some(&stick)),
            sibling_stick
        );
        assert_eq!(from_persisted_path(home_file, Some(&stick)), home_file);
    }

    #[cfg(unix)]
    #[test]
    fn test_relative_path_follows_a_mount_point_change() {
        let first_mount = layout("/media/user/STICK/Fulgur", "/media/user/STICK");
        let second_mount = layout("/run/media/user/STICK/Fulgur", "/run/media/user/STICK");

        let persisted = to_persisted_path(
            Path::new("/media/user/STICK/Docs/notes.md"),
            Some(&first_mount),
        );

        assert_eq!(
            from_persisted_path(&persisted, Some(&second_mount)),
            PathBuf::from("/run/media/user/STICK/Docs/notes.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_round_trip_is_identity_and_deterministic() {
        let stick = layout("/media/user/STICK/Apps/Fulgur", "/media/user/STICK");
        for original in [
            "/media/user/STICK/Apps/Fulgur/data/scratch.txt",
            "/media/user/STICK/Apps/other/readme.md",
            "/media/user/STICK/top.md",
            "/etc/hosts",
        ] {
            let original = Path::new(original);
            let persisted = to_persisted_path(original, Some(&stick));

            assert_eq!(to_persisted_path(original, Some(&stick)), persisted);
            assert_eq!(from_persisted_path(&persisted, Some(&stick)), original);
        }
    }

    #[cfg(windows)]
    #[test]
    fn test_same_drive_path_becomes_relative_and_follows_a_drive_letter_change() {
        let on_e = layout(r"E:\Fulgur", r"E:\");
        let on_f = layout(r"F:\Fulgur", r"F:\");

        let persisted = to_persisted_path(Path::new(r"E:\Docs\notes.md"), Some(&on_e));

        assert_eq!(persisted, PathBuf::from(r"..\Docs\notes.md"));
        assert_eq!(
            from_persisted_path(&persisted, Some(&on_f)),
            PathBuf::from(r"F:\Docs\notes.md")
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_drive_letter_case_does_not_change_the_volume() {
        let on_e = layout(r"E:\Fulgur", r"E:\");

        assert_eq!(
            to_persisted_path(Path::new(r"e:\Docs\notes.md"), Some(&on_e)),
            PathBuf::from(r"..\Docs\notes.md")
        );
    }

    #[cfg(windows)]
    #[test]
    fn test_other_drive_and_unc_paths_stay_absolute() {
        let on_e = layout(r"E:\Fulgur", r"E:\");
        let other_drive = Path::new(r"C:\Users\user\notes.md");
        let unc = Path::new(r"\\server\share\notes.md");

        assert_eq!(to_persisted_path(other_drive, Some(&on_e)), other_drive);
        assert_eq!(to_persisted_path(unc, Some(&on_e)), unc);
    }
}
