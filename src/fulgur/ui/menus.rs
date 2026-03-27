use gpui::*;
use std::path::PathBuf;

use crate::fulgur::{Fulgur, utils::updater::check_for_updates};
#[cfg(not(target_os = "macos"))]
use gpui_component::GlobalState;
use gpui_component::{WindowExt, notification::NotificationType};

actions!(
    fulgur,
    [
        NoneAction,
        About,
        Quit,
        CloseWindow,
        NewFile,
        NewWindow,
        OpenFile,
        OpenPath,
        SaveFileAs,
        SaveFile,
        CloseFile,
        CloseAllFiles,
        FindInFile,
        SettingsTab,
        GetTheme,
        NextTab,
        PreviousTab,
        JumpToLine,
        ClearRecentFiles,
        SelectTheme,
        CheckForUpdates,
        PrintFile,
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct OpenRecentFile(pub PathBuf);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct DockActivateTab(pub PathBuf);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct DockActivateTabByTitle(pub SharedString);

/// Keybinding action target used to map shortcuts to dispatchable Fulgur actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum KeybindingDispatchAction {
    OpenFile,
    NewFile,
    OpenPath,
    NewWindow,
    CloseFile,
    CloseAllFiles,
    Quit,
    SaveFile,
    SaveFileAs,
    FindInFile,
    NextTab,
    PreviousTab,
    JumpToLine,
    PrintFile,
}

/// A platform keybinding dispatch specification used to build runtime keybindings.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct KeybindingDispatchSpec {
    keystroke: &'static str,
    action: KeybindingDispatchAction,
}

impl KeybindingDispatchSpec {
    /// Create a keybinding dispatch specification.
    ///
    /// ### Parameters:
    /// - `keystroke`: The key combination string consumed by GPUI.
    /// - `action`: The action to dispatch for this keybinding.
    ///
    /// ### Returns:
    /// `Self`: The new keybinding dispatch specification.
    const fn new(keystroke: &'static str, action: KeybindingDispatchAction) -> Self {
        Self { keystroke, action }
    }

    /// Convert this dispatch specification into a GPUI keybinding instance.
    ///
    /// ### Returns:
    /// `KeyBinding`: The runtime keybinding bound to the configured action.
    fn into_key_binding(self) -> KeyBinding {
        match self.action {
            KeybindingDispatchAction::OpenFile => KeyBinding::new(self.keystroke, OpenFile, None),
            KeybindingDispatchAction::NewFile => KeyBinding::new(self.keystroke, NewFile, None),
            KeybindingDispatchAction::OpenPath => KeyBinding::new(self.keystroke, OpenPath, None),
            KeybindingDispatchAction::NewWindow => KeyBinding::new(self.keystroke, NewWindow, None),
            KeybindingDispatchAction::CloseFile => KeyBinding::new(self.keystroke, CloseFile, None),
            KeybindingDispatchAction::CloseAllFiles => {
                KeyBinding::new(self.keystroke, CloseAllFiles, None)
            }
            KeybindingDispatchAction::Quit => KeyBinding::new(self.keystroke, Quit, None),
            KeybindingDispatchAction::SaveFile => KeyBinding::new(self.keystroke, SaveFile, None),
            KeybindingDispatchAction::SaveFileAs => {
                KeyBinding::new(self.keystroke, SaveFileAs, None)
            }
            KeybindingDispatchAction::FindInFile => {
                KeyBinding::new(self.keystroke, FindInFile, None)
            }
            KeybindingDispatchAction::NextTab => KeyBinding::new(self.keystroke, NextTab, None),
            KeybindingDispatchAction::PreviousTab => {
                KeyBinding::new(self.keystroke, PreviousTab, None)
            }
            KeybindingDispatchAction::JumpToLine => {
                KeyBinding::new(self.keystroke, JumpToLine, None)
            }
            KeybindingDispatchAction::PrintFile => KeyBinding::new(self.keystroke, PrintFile, None),
        }
    }
}

/// Build platform-specific keybinding dispatch specifications.
///
/// ### Returns:
/// `Vec<KeybindingDispatchSpec>`: The complete keybinding-to-action mapping for this platform.
fn default_keybinding_dispatch_specs() -> Vec<KeybindingDispatchSpec> {
    vec![
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-o", KeybindingDispatchAction::OpenFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-o", KeybindingDispatchAction::OpenFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-n", KeybindingDispatchAction::NewFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-n", KeybindingDispatchAction::NewFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-o", KeybindingDispatchAction::OpenPath),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-o", KeybindingDispatchAction::OpenPath),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-n", KeybindingDispatchAction::NewWindow),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-n", KeybindingDispatchAction::NewWindow),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-w", KeybindingDispatchAction::CloseFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-w", KeybindingDispatchAction::CloseFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-w", KeybindingDispatchAction::CloseAllFiles),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-w", KeybindingDispatchAction::CloseAllFiles),
        KeybindingDispatchSpec::new("cmd-q", KeybindingDispatchAction::Quit),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("alt-f4", KeybindingDispatchAction::Quit),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-s", KeybindingDispatchAction::SaveFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-s", KeybindingDispatchAction::SaveFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-s", KeybindingDispatchAction::SaveFileAs),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-s", KeybindingDispatchAction::SaveFileAs),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-f", KeybindingDispatchAction::FindInFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-f", KeybindingDispatchAction::FindInFile),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-right", KeybindingDispatchAction::NextTab),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-right", KeybindingDispatchAction::NextTab),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-left", KeybindingDispatchAction::PreviousTab),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-left", KeybindingDispatchAction::PreviousTab),
        KeybindingDispatchSpec::new("ctrl-g", KeybindingDispatchAction::JumpToLine),
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-p", KeybindingDispatchAction::PrintFile),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-p", KeybindingDispatchAction::PrintFile),
    ]
}

/// Build the default runtime keybindings for the application.
///
/// ### Returns:
/// `Vec<KeyBinding>`: The platform-specific list of GPUI keybindings.
pub fn build_default_key_bindings() -> Vec<KeyBinding> {
    default_keybinding_dispatch_specs()
        .into_iter()
        .map(KeybindingDispatchSpec::into_key_binding)
        .collect()
}

/// A single tab entry for the dock menu, carrying the display name and the action to fire
#[cfg(target_os = "macos")]
pub enum DockMenuTab {
    /// File-backed editor tab — use path for reliable cross-window lookup
    File { name: SharedString, path: PathBuf },
    /// Non-file tab (settings, untitled editor, markdown preview) — use title for lookup
    Titled {
        name: SharedString,
        title: SharedString,
    },
}

/// Build the menus for the Fulgur instance
///
/// ### Arguments
/// - `recent_files`: The list of recent files to display
/// - `update_link`: The optional link to the update
///
/// ### Returns
/// - `Vec<Menu>`: The menus for the Fulgur instance
pub fn build_menus(recent_files: &[PathBuf], update_link: Option<String>) -> Vec<Menu> {
    let recent_files_items = if recent_files.is_empty() {
        vec![MenuItem::action("No recent files", NoneAction)]
    } else {
        let mut items: Vec<MenuItem> = recent_files
            .iter()
            .map(|file| {
                MenuItem::action(
                    file.display().to_string(),
                    OpenRecentFile(file.to_path_buf()),
                )
            })
            .collect();
        items.push(MenuItem::Separator);
        items.push(MenuItem::action("Clear recent files", ClearRecentFiles));
        items
    };
    vec![
        Menu {
            name: "Fulgur".into(),
            items: vec![
                MenuItem::action("About Fulgur", About),
                if update_link.is_some() {
                    MenuItem::action("Update available", CheckForUpdates)
                } else {
                    MenuItem::action("Check for updates", CheckForUpdates)
                },
                MenuItem::Separator,
                MenuItem::action("Settings", SettingsTab),
                MenuItem::action("Select theme", SelectTheme),
                MenuItem::action("Get more themes...", GetTheme),
                MenuItem::Separator,
                MenuItem::action("Close Window", CloseWindow),
                MenuItem::action("Quit", Quit),
            ],
        },
        Menu {
            name: "File".into(),
            items: vec![
                MenuItem::action("New", NewFile),
                MenuItem::action("New Window", NewWindow),
                MenuItem::action("Open...", OpenFile),
                MenuItem::action("Open from path...", OpenPath),
                MenuItem::Submenu(Menu {
                    name: "Recent Files".into(),
                    items: recent_files_items,
                }),
                MenuItem::separator(),
                MenuItem::action("Save", SaveFile),
                MenuItem::action("Save as...", SaveFileAs),
                MenuItem::separator(),
                MenuItem::action("Print...", PrintFile),
                MenuItem::separator(),
                MenuItem::action("Close file", CloseFile),
                MenuItem::action("Close all files", CloseAllFiles),
            ],
        },
        Menu {
            name: "Edit".into(),
            items: vec![
                MenuItem::action("Undo", gpui_component::input::Undo),
                MenuItem::action("Redo", gpui_component::input::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", gpui_component::input::Cut),
                MenuItem::action("Copy", gpui_component::input::Copy),
                MenuItem::action("Paste", gpui_component::input::Paste),
                MenuItem::separator(),
                MenuItem::action("Find & Replace", FindInFile),
            ],
        },
        Menu {
            name: "Go".into(),
            items: vec![
                MenuItem::action("Next Tab", NextTab),
                MenuItem::action("Previous Tab", PreviousTab),
                MenuItem::Separator,
                MenuItem::action("Jump to line", JumpToLine),
            ],
        },
    ]
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
            .map(|file| {
                MenuItem::action(
                    file.display().to_string(),
                    OpenRecentFile(file.to_path_buf()),
                )
            })
            .collect();
        items.push(MenuItem::Submenu(Menu {
            name: "Recent Files".into(),
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

impl Fulgur {
    /// Set the application menus and sync them to the AppMenuBar on non-macOS platforms.
    ///
    /// ### Arguments
    /// - `menus`: The menus to set
    /// - `cx`: The application context
    pub fn update_menus(&mut self, menus: Vec<Menu>, cx: &mut Context<Self>) {
        cx.set_menus(menus);
        #[cfg(not(target_os = "macos"))]
        {
            if let Some(owned_menus) = cx.get_menus() {
                GlobalState::global_mut(cx).set_app_menus(owned_menus);
            }
            self.title_bar
                .update(cx, |tb, cx| tb.reload_app_menu_bar(cx));
        }
    }

    /// Check for updates, open the download page in the browser if an update is available, update the menus to show the update available action and trigger notifications
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn check_for_updates(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(update_info) = self.shared_state(cx).update_info.lock().as_ref() {
            match open::that(update_info.download_url.clone()) {
                Ok(_) => {
                    log::debug!("Successfully opened browser");
                }
                Err(e) => {
                    log::error!("Failed to open browser: {}", e);
                }
            }
            return;
        }
        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, window| {
            log::debug!("Checking for updates");
            let current_version = env!("CARGO_PKG_VERSION");
            log::debug!("Current version: {}", current_version);
            let update_info = bg
                .spawn(async move {
                    check_for_updates(current_version.to_string())
                        .ok()
                        .flatten()
                })
                .await;
            window
                .update(|window, cx| {
                    if let Some(new_update_info) = update_info {
                        let current_ver = new_update_info.current_version.clone();
                        let latest_ver = new_update_info.latest_version.clone();
                        let download_url = new_update_info.download_url.clone();
                        let _ = view.update(cx, |this, cx| {
                            {
                                let mut update_info = this.shared_state(cx).update_info.lock();
                                *update_info = Some(new_update_info);
                            }
                            let menus = build_menus(
                                this.settings.recent_files.get_files(),
                                Some(download_url),
                            );
                            this.update_menus(menus, cx);
                            cx.notify();
                        });
                        log::info!("Update available: {} -> {}", current_ver, latest_ver);
                    } else {
                        let notification = SharedString::from("No update found");
                        window.push_notification((NotificationType::Info, notification), cx);
                    }
                })
                .ok();
        })
        .detach();
    }
}

#[cfg(test)]
mod tests {
    use super::{
        KeybindingDispatchAction, build_default_key_bindings, default_keybinding_dispatch_specs,
    };
    use core::prelude::v1::test;
    use std::collections::HashSet;

    fn has_binding(
        specs: &[super::KeybindingDispatchSpec],
        keystroke: &str,
        action: KeybindingDispatchAction,
    ) -> bool {
        specs
            .iter()
            .any(|spec| spec.keystroke == keystroke && spec.action == action)
    }

    #[test]
    fn test_default_keybinding_dispatch_specs_include_core_editor_actions() {
        let specs = default_keybinding_dispatch_specs();

        #[cfg(target_os = "macos")]
        assert!(has_binding(
            &specs,
            "cmd-o",
            KeybindingDispatchAction::OpenFile
        ));
        #[cfg(not(target_os = "macos"))]
        assert!(has_binding(
            &specs,
            "ctrl-o",
            KeybindingDispatchAction::OpenFile
        ));

        #[cfg(target_os = "macos")]
        assert!(has_binding(
            &specs,
            "cmd-s",
            KeybindingDispatchAction::SaveFile
        ));
        #[cfg(not(target_os = "macos"))]
        assert!(has_binding(
            &specs,
            "ctrl-s",
            KeybindingDispatchAction::SaveFile
        ));

        assert!(has_binding(
            &specs,
            "ctrl-g",
            KeybindingDispatchAction::JumpToLine
        ));
    }

    #[test]
    fn test_default_keybinding_dispatch_specs_include_platform_quit_shortcuts() {
        let specs = default_keybinding_dispatch_specs();
        assert!(has_binding(&specs, "cmd-q", KeybindingDispatchAction::Quit));

        #[cfg(not(target_os = "macos"))]
        assert!(has_binding(
            &specs,
            "alt-f4",
            KeybindingDispatchAction::Quit
        ));
    }

    #[test]
    fn test_default_keybinding_dispatch_specs_do_not_duplicate_same_shortcut_action() {
        let specs = default_keybinding_dispatch_specs();
        let unique_pairs: HashSet<(&str, KeybindingDispatchAction)> = specs
            .iter()
            .map(|spec| (spec.keystroke, spec.action))
            .collect();

        assert_eq!(unique_pairs.len(), specs.len());
    }

    #[test]
    fn test_build_default_key_bindings_matches_dispatch_spec_count() {
        let specs = default_keybinding_dispatch_specs();
        let keybindings = build_default_key_bindings();
        assert_eq!(keybindings.len(), specs.len());
    }
}
