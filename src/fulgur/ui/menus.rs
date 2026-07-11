use crate::fulgur::{
    Fulgur,
    utils::updater::{check_for_updates, is_valid_release_page_url},
};
use gpui::{Context, KeyBinding, Menu, MenuItem, SharedString, Window, actions};
#[cfg(not(target_os = "macos"))]
use gpui_component::GlobalState;
use gpui_component::{WindowExt, notification::NotificationType};
use gpui_macros::Action;
use std::path::PathBuf;

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
        OpenRemote,
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
        ToggleColorPicker,
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

/// Key context set on the application content element, used to scope keybindings
/// so they do not fire while focus is in a modal layer (gpui-component sets its own
/// "Dialog" and "Sheet" contexts there).
pub const KEY_CONTEXT_FULGUR: &str = "Fulgur";

/// Context predicate for keybindings scoped to the application content.
const SCOPED_BINDING_PREDICATE: &str = "Fulgur || (Fulgur > Input)";

/// Keybinding action target used to map shortcuts to dispatchable Fulgur actions.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
enum KeybindingDispatchAction {
    OpenFile,
    NewFile,
    OpenPath,
    OpenRemote,
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
    ToggleColorPicker,
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
    /// `KeyBinding`: The runtime keybinding bound to the configured action, scoped
    /// to this action's key context.
    fn into_key_binding(self) -> KeyBinding {
        let context = self.action.key_context();
        match self.action {
            KeybindingDispatchAction::OpenFile => {
                KeyBinding::new(self.keystroke, OpenFile, context)
            }
            KeybindingDispatchAction::NewFile => KeyBinding::new(self.keystroke, NewFile, context),
            KeybindingDispatchAction::OpenPath => {
                KeyBinding::new(self.keystroke, OpenPath, context)
            }
            KeybindingDispatchAction::OpenRemote => {
                KeyBinding::new(self.keystroke, OpenRemote, context)
            }
            KeybindingDispatchAction::NewWindow => {
                KeyBinding::new(self.keystroke, NewWindow, context)
            }
            KeybindingDispatchAction::CloseFile => {
                KeyBinding::new(self.keystroke, CloseFile, context)
            }
            KeybindingDispatchAction::CloseAllFiles => {
                KeyBinding::new(self.keystroke, CloseAllFiles, context)
            }
            KeybindingDispatchAction::Quit => KeyBinding::new(self.keystroke, Quit, context),
            KeybindingDispatchAction::SaveFile => {
                KeyBinding::new(self.keystroke, SaveFile, context)
            }
            KeybindingDispatchAction::SaveFileAs => {
                KeyBinding::new(self.keystroke, SaveFileAs, context)
            }
            KeybindingDispatchAction::FindInFile => {
                KeyBinding::new(self.keystroke, FindInFile, context)
            }
            KeybindingDispatchAction::NextTab => KeyBinding::new(self.keystroke, NextTab, context),
            KeybindingDispatchAction::PreviousTab => {
                KeyBinding::new(self.keystroke, PreviousTab, context)
            }
            KeybindingDispatchAction::JumpToLine => {
                KeyBinding::new(self.keystroke, JumpToLine, context)
            }
            KeybindingDispatchAction::PrintFile => {
                KeyBinding::new(self.keystroke, PrintFile, context)
            }
            KeybindingDispatchAction::ToggleColorPicker => {
                KeyBinding::new(self.keystroke, ToggleColorPicker, context)
            }
        }
    }
}

impl KeybindingDispatchAction {
    /// Get the key context under which this action's binding is active.
    ///
    /// ### Returns
    /// - `Some(&'static str)`: The key context the binding is scoped to
    /// - `None`: The binding is global
    const fn key_context(self) -> Option<&'static str> {
        match self {
            Self::OpenFile
            | Self::NewFile
            | Self::OpenPath
            | Self::OpenRemote
            | Self::NewWindow
            | Self::Quit => None,
            Self::CloseFile
            | Self::CloseAllFiles
            | Self::SaveFile
            | Self::SaveFileAs
            | Self::FindInFile
            | Self::NextTab
            | Self::PreviousTab
            | Self::JumpToLine
            | Self::PrintFile
            | Self::ToggleColorPicker => Some(SCOPED_BINDING_PREDICATE),
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
        KeybindingDispatchSpec::new("cmd-shift-r", KeybindingDispatchAction::OpenRemote),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-r", KeybindingDispatchAction::OpenRemote),
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
        #[cfg(target_os = "macos")]
        KeybindingDispatchSpec::new("cmd-shift-c", KeybindingDispatchAction::ToggleColorPicker),
        #[cfg(not(target_os = "macos"))]
        KeybindingDispatchSpec::new("ctrl-shift-c", KeybindingDispatchAction::ToggleColorPicker),
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

/// A single tab entry for the dock/taskbar menu, carrying the display name and the action to fire
#[cfg(any(target_os = "macos", target_os = "windows"))]
pub enum DockMenuTab {
    /// File-backed editor tab, use path for reliable cross-window lookup
    File { name: SharedString, path: PathBuf },
    /// Non-file tab (settings, untitled editor, markdown preview), use title for lookup
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
pub fn build_menus(recent_files: &[PathBuf], update_link: Option<&str>) -> Vec<Menu> {
    let recent_files_items = if recent_files.is_empty() {
        vec![MenuItem::action("No recent files", NoneAction)]
    } else {
        let mut items: Vec<MenuItem> = recent_files
            .iter()
            .map(|file| MenuItem::action(file.display().to_string(), OpenRecentFile(file.clone())))
            .collect();
        items.push(MenuItem::Separator);
        items.push(MenuItem::action("Clear recent files", ClearRecentFiles));
        items
    };
    vec![
        Menu {
            name: "Fulgur".into(),
            disabled: false,
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
            disabled: false,
            items: vec![
                MenuItem::action("New", NewFile),
                MenuItem::action("New Window", NewWindow),
                MenuItem::action("Open...", OpenFile),
                MenuItem::action("Open from path...", OpenPath),
                MenuItem::action("Open remote file...", OpenRemote),
                MenuItem::Submenu(Menu {
                    name: "Recent Files".into(),
                    disabled: false,
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
            disabled: false,
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
            name: "View".into(),
            disabled: false,
            items: vec![
                MenuItem::action("Color picker", ToggleColorPicker),
                MenuItem::separator(),
            ],
        },
        Menu {
            name: "Go".into(),
            disabled: false,
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

impl Fulgur {
    /// Set the application menus and sync them to the `AppMenuBar` on non-macOS platforms.
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
            self.title_bar.update(
                cx,
                super::bars::titlebar::CustomTitleBar::reload_app_menu_bar,
            );
        }
    }

    /// Check for updates, open the download page in the browser if an update is available, update the menus to show the update available action and trigger notifications
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn check_for_updates(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(update_info) = Fulgur::shared_state(cx).update_info.lock().as_ref() {
            let url = update_info.download_url.clone();
            if !is_valid_release_page_url(&url) {
                log::error!("Refusing to open non-canonical update URL: {url}");
                return;
            }
            match open::that(url) {
                Ok(()) => {
                    log::debug!("Successfully opened browser");
                }
                Err(e) => {
                    log::error!("Failed to open browser: {e}");
                }
            }
            return;
        }
        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, window| {
            log::debug!("Checking for updates");
            let current_version = env!("CARGO_PKG_VERSION");
            log::debug!("Current version: {current_version}");
            let update_info = bg
                .spawn(async move { check_for_updates(current_version).ok().flatten() })
                .await;
            window
                .update(|window, cx| {
                    if let Some(new_update_info) = update_info {
                        let current_ver = new_update_info.current_version.clone();
                        let latest_ver = new_update_info.latest_version.clone();
                        let download_url = new_update_info.download_url.clone();
                        let _ = view.update(cx, |this, cx| {
                            {
                                let mut update_info = Fulgur::shared_state(cx).update_info.lock();
                                *update_info = Some(new_update_info);
                            }
                            let menus = build_menus(
                                this.settings.recent_files.get_files(),
                                Some(download_url.as_str()),
                            );
                            this.update_menus(menus, cx);
                            cx.notify();
                        });
                        log::info!("Update available: {current_ver} -> {latest_ver}");
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
    #[cfg(target_os = "macos")]
    use super::{DockMenuTab, build_dock_menu};
    use super::{
        KeybindingDispatchAction, build_default_key_bindings, default_keybinding_dispatch_specs,
    };
    use core::prelude::v1::test;
    #[cfg(target_os = "macos")]
    use gpui::MenuItem;
    #[cfg(target_os = "macos")]
    use gpui::SharedString;
    use std::collections::HashSet;
    #[cfg(target_os = "macos")]
    use std::path::PathBuf;

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

    #[test]
    fn test_scoped_predicate_matches_editor_input_depth_but_not_modal_inputs() {
        let scoped =
            gpui::KeyBindingContextPredicate::parse(super::SCOPED_BINDING_PREDICATE).unwrap();
        let input = gpui::KeyBindingContextPredicate::parse("Input").unwrap();

        let editor_stack = [
            gpui::KeyContext::parse("Fulgur").unwrap(),
            gpui::KeyContext::parse("Input").unwrap(),
        ];
        assert_eq!(
            scoped.depth_of(&editor_stack),
            input.depth_of(&editor_stack),
            "scoped bindings must match at the same depth as gpui-component's \
             Input bindings, so Fulgur's later registration wins the tie"
        );

        let dialog_stack = [
            gpui::KeyContext::parse("Dialog").unwrap(),
            gpui::KeyContext::parse("Input").unwrap(),
        ];
        assert_eq!(
            scoped.depth_of(&dialog_stack),
            None,
            "scoped bindings must not fire inside modal inputs"
        );

        let content_stack = [gpui::KeyContext::parse("Fulgur").unwrap()];
        assert!(
            scoped.depth_of(&content_stack).is_some(),
            "scoped bindings must fire when the app content itself is focused"
        );
    }

    #[test]
    fn test_window_level_actions_are_global_and_editor_actions_are_scoped() {
        let window_level = [
            KeybindingDispatchAction::OpenFile,
            KeybindingDispatchAction::NewFile,
            KeybindingDispatchAction::OpenPath,
            KeybindingDispatchAction::OpenRemote,
            KeybindingDispatchAction::NewWindow,
            KeybindingDispatchAction::Quit,
        ];
        for action in window_level {
            assert_eq!(action.key_context(), None, "{action:?} should be global");
        }

        let editor_scoped = [
            KeybindingDispatchAction::CloseFile,
            KeybindingDispatchAction::CloseAllFiles,
            KeybindingDispatchAction::SaveFile,
            KeybindingDispatchAction::SaveFileAs,
            KeybindingDispatchAction::FindInFile,
            KeybindingDispatchAction::NextTab,
            KeybindingDispatchAction::PreviousTab,
            KeybindingDispatchAction::JumpToLine,
            KeybindingDispatchAction::PrintFile,
            KeybindingDispatchAction::ToggleColorPicker,
        ];
        for action in editor_scoped {
            assert_eq!(
                action.key_context(),
                Some(super::SCOPED_BINDING_PREDICATE),
                "{action:?} should be scoped to the application content"
            );
        }
    }

    #[cfg(target_os = "macos")]
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

    #[cfg(target_os = "macos")]
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
