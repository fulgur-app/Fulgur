#[cfg(target_os = "macos")]
use super::actions::{DockActivateTab, DockActivateTabByTitle, NewFile, NewWindow, OpenRecentFile};
use gpui::SharedString;
#[cfg(target_os = "macos")]
use gpui::{Menu, MenuItem};
use std::path::PathBuf;

/// A single tab entry for the dock/taskbar menu, carrying the display name and the action to fire
pub enum DockMenuTab {
    /// File-backed editor tab, use path for reliable cross-window lookup
    File { name: SharedString, path: PathBuf },
    /// Non-file tab (settings, untitled editor, markdown preview), use title for lookup
    Titled {
        name: SharedString,
        title: SharedString,
    },
}

/// Build the macOS dock menu (right-click on dock icon)
///
/// Shows recent files in a submenu, then open tabs grouped by window (separated by dividers),
/// then new tab/window actions.
///
/// ### Arguments
/// - `windows`: Open tabs grouped by window; each inner slice represents one window's tabs
/// - `recent_files`: List of recent file paths (most recent first)
///
/// ### Returns
/// - `Vec<MenuItem>`: The dock menu items
#[cfg(target_os = "macos")]
pub fn build_dock_menu(windows: &[Vec<DockMenuTab>], recent_files: &[PathBuf]) -> Vec<MenuItem> {
    let mut items = Vec::new();
    if !recent_files.is_empty() {
        let recent_items: Vec<MenuItem> = recent_files
            .iter()
            .map(|file| MenuItem::action(file.display().to_string(), OpenRecentFile(file.clone())))
            .collect();
        items.push(MenuItem::Submenu(Menu {
            name: "Recent Files".into(),
            disabled: false,
            items: recent_items,
        }));
        items.push(MenuItem::Separator);
    }
    let non_empty_windows: Vec<&Vec<DockMenuTab>> =
        windows.iter().filter(|w| !w.is_empty()).collect();
    if !non_empty_windows.is_empty() {
        for (i, window_tabs) in non_empty_windows.iter().enumerate() {
            if i > 0 {
                items.push(MenuItem::Separator);
            }
            for tab in *window_tabs {
                let menu_item = match tab {
                    DockMenuTab::File { name, path } => {
                        MenuItem::action(name.clone(), DockActivateTab(path.clone()))
                    }
                    DockMenuTab::Titled { name, title } => {
                        MenuItem::action(name.clone(), DockActivateTabByTitle(title.clone()))
                    }
                };
                items.push(menu_item);
            }
        }
        items.push(MenuItem::Separator);
    }
    items.push(MenuItem::action("New Tab", NewFile));
    items.push(MenuItem::action("New Window", NewWindow));
    items
}

#[cfg(all(test, target_os = "macos"))]
mod tests {
    use super::{DockMenuTab, build_dock_menu};
    use core::prelude::v1::test;
    use gpui::MenuItem;
    use gpui::SharedString;
    use std::path::PathBuf;

    #[test]
    fn test_build_dock_menu_with_recent_and_window_groups_filters_empty_windows() {
        let windows = vec![
            vec![DockMenuTab::Titled {
                name: SharedString::from("Untitled"),
                title: SharedString::from("Untitled"),
            }],
            vec![],
            vec![DockMenuTab::File {
                name: SharedString::from("notes.md"),
                path: PathBuf::from("/tmp/notes.md"),
            }],
        ];
        let recent = vec![
            PathBuf::from("/tmp/recent-a.rs"),
            PathBuf::from("/tmp/recent-b.rs"),
        ];
        let items = build_dock_menu(&windows, &recent);
        assert!(
            matches!(items.first(), Some(MenuItem::Submenu(_))),
            "dock menu should begin with the recent-files submenu when recents exist"
        );
        let separator_count = items
            .iter()
            .filter(|item| matches!(item, MenuItem::Separator))
            .count();
        assert_eq!(
            separator_count, 3,
            "expected separators: after recents, between window groups, and before new actions"
        );
        assert_eq!(
            items.len(),
            8,
            "expected submenu+separator, two tab actions with one group separator, trailing separator, and two creation actions"
        );
    }

    #[test]
    fn test_build_dock_menu_without_recent_or_tabs_returns_creation_actions_only() {
        let items = build_dock_menu(&[], &[]);
        assert_eq!(items.len(), 2);
        assert!(
            items
                .iter()
                .all(|item| !matches!(item, MenuItem::Separator | MenuItem::Submenu(_))),
            "when no recents or tabs exist, dock menu should only include direct action items"
        );
    }
}
