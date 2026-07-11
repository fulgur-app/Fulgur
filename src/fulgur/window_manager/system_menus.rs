//! App-scope rebuilding of the OS-level menus (macOS dock menu, Windows jump list).

use gpui::SharedString;
use std::path::PathBuf;

/// Menu-relevant snapshot of one tab, published per window to the `WindowManager`
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct WindowMenuTab {
    /// File backing the tab, when it has one (used for reliable cross-window lookup)
    pub path: Option<PathBuf>,
    /// Tab title (used for lookup of non-file tabs)
    pub title: SharedString,
}

/// Register the app-scope observers that keep the OS menus in sync.
///
/// ### Arguments
/// - `cx`: The application context
pub fn init(cx: &mut gpui::App) {
    #[cfg(any(target_os = "macos", target_os = "windows"))]
    {
        use crate::fulgur::shared_state::SharedAppState;
        use crate::fulgur::window_manager::WindowManager;
        use std::cell::RefCell;
        use std::rc::Rc;

        let last_inputs: Rc<RefCell<Option<SystemMenuInputs>>> = Rc::new(RefCell::new(None));
        let inputs_for_manager = last_inputs.clone();
        cx.observe_global::<WindowManager>(move |cx| {
            rebuild_if_inputs_changed(cx, &inputs_for_manager);
        })
        .detach();
        cx.observe_global::<SharedAppState>(move |cx| {
            rebuild_if_inputs_changed(cx, &last_inputs);
        })
        .detach();
    }
    #[cfg(not(any(target_os = "macos", target_os = "windows")))]
    {
        let _ = cx;
    }
}

/// Combined inputs of the system menus, compared between rebuilds
#[cfg(any(target_os = "macos", target_os = "windows"))]
#[derive(PartialEq, Eq)]
struct SystemMenuInputs {
    /// Published tab snapshots of every window, in registration order
    windows: Vec<Vec<WindowMenuTab>>,
    /// Recent file paths, most recent first
    recent_files: Vec<PathBuf>,
}

/// Rebuild the dock menu / jump list when the menu inputs changed.
///
/// ### Arguments
/// - `cx`: The application context
/// - `last_inputs`: Inputs of the previous build, used to skip no-op rebuilds
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn rebuild_if_inputs_changed(
    cx: &mut gpui::App,
    last_inputs: &std::cell::RefCell<Option<SystemMenuInputs>>,
) {
    use crate::fulgur::shared_state::SharedAppState;
    use crate::fulgur::window_manager::WindowManager;

    let inputs = SystemMenuInputs {
        windows: cx.global::<WindowManager>().ordered_window_menu_tabs(),
        recent_files: cx.global::<SharedAppState>().settings.get_recent_files(),
    };
    if last_inputs.borrow().as_ref() == Some(&inputs) {
        return;
    }
    let menu_tabs = to_dock_menu_tabs(&inputs.windows);
    #[cfg(target_os = "macos")]
    cx.set_dock_menu(crate::fulgur::ui::menus::build_dock_menu(
        &menu_tabs,
        &inputs.recent_files,
    ));
    #[cfg(target_os = "windows")]
    crate::fulgur::utils::jump_list::update_windows_jump_list(&menu_tabs, &inputs.recent_files);
    *last_inputs.borrow_mut() = Some(inputs);
}

/// Convert published tab snapshots to dock menu entries with display names.
///
/// ### Arguments
/// - `windows`: Published tab snapshots of every window
///
/// ### Returns
/// - `Vec<Vec<DockMenuTab>>`: Menu entries grouped by window
#[cfg(any(target_os = "macos", target_os = "windows"))]
fn to_dock_menu_tabs(
    windows: &[Vec<WindowMenuTab>],
) -> Vec<Vec<crate::fulgur::ui::menus::DockMenuTab>> {
    use crate::fulgur::ui::menus::DockMenuTab;

    let all_file_paths: Vec<&PathBuf> = windows
        .iter()
        .flat_map(|window| window.iter())
        .filter_map(|tab| tab.path.as_ref())
        .collect();
    let display_name_for_path = |path: &PathBuf| -> SharedString {
        let filename = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Untitled");
        let has_duplicate = all_file_paths.iter().any(|other_path| {
            *other_path != path
                && (other_path.file_name().and_then(|n| n.to_str()) == Some(filename))
        });
        if has_duplicate
            && let Some(parent_name) = path
                .parent()
                .and_then(|p| p.file_name())
                .and_then(|n| n.to_str())
        {
            return SharedString::from(format!("{filename} (../{parent_name})"));
        }
        SharedString::from(filename.to_string())
    };
    windows
        .iter()
        .map(|window| {
            window
                .iter()
                .map(|tab| match &tab.path {
                    Some(path) => DockMenuTab::File {
                        name: display_name_for_path(path),
                        path: path.clone(),
                    },
                    None => DockMenuTab::Titled {
                        name: tab.title.clone(),
                        title: tab.title.clone(),
                    },
                })
                .collect()
        })
        .collect()
}

#[cfg(all(test, any(target_os = "macos", target_os = "windows")))]
mod tests {
    use super::{WindowMenuTab, to_dock_menu_tabs};
    use crate::fulgur::ui::menus::DockMenuTab;
    use std::path::PathBuf;

    /// Build a file-backed snapshot tab for tests.
    fn file_tab(path: &str) -> WindowMenuTab {
        WindowMenuTab {
            path: Some(PathBuf::from(path)),
            title: gpui::SharedString::from(
                PathBuf::from(path)
                    .file_name()
                    .and_then(|n| n.to_str())
                    .unwrap_or_default()
                    .to_string(),
            ),
        }
    }

    #[test]
    fn test_to_dock_menu_tabs_uses_plain_filename_when_unique() {
        let windows = vec![vec![file_tab("/projects/alpha/main.rs")]];
        let converted = to_dock_menu_tabs(&windows);
        match &converted[0][0] {
            DockMenuTab::File { name, .. } => {
                assert_eq!(
                    name.as_ref(),
                    "main.rs",
                    "unique filenames should not carry a parent-directory suffix"
                );
            }
            DockMenuTab::Titled { .. } => panic!("file-backed tab should convert to File entry"),
        }
    }

    #[test]
    fn test_to_dock_menu_tabs_disambiguates_duplicate_filenames_across_windows() {
        let windows = vec![
            vec![file_tab("/projects/alpha/main.rs")],
            vec![file_tab("/projects/beta/main.rs")],
        ];
        let converted = to_dock_menu_tabs(&windows);
        match (&converted[0][0], &converted[1][0]) {
            (DockMenuTab::File { name: first, .. }, DockMenuTab::File { name: second, .. }) => {
                assert_eq!(
                    first.as_ref(),
                    "main.rs (../alpha)",
                    "duplicate filenames should be disambiguated with the parent directory"
                );
                assert_eq!(
                    second.as_ref(),
                    "main.rs (../beta)",
                    "duplicate filenames should be disambiguated with the parent directory"
                );
            }
            _ => panic!("file-backed tabs should convert to File entries"),
        }
    }

    #[test]
    fn test_to_dock_menu_tabs_maps_titled_tabs_to_title_lookup() {
        let windows = vec![vec![WindowMenuTab {
            path: None,
            title: gpui::SharedString::from("Settings"),
        }]];
        let converted = to_dock_menu_tabs(&windows);
        match &converted[0][0] {
            DockMenuTab::Titled { name, title } => {
                assert_eq!(name.as_ref(), "Settings");
                assert_eq!(title.as_ref(), "Settings");
            }
            DockMenuTab::File { .. } => panic!("path-less tab should convert to Titled entry"),
        }
    }
}
