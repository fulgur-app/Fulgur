use gpui::*;
use std::path::PathBuf;

use crate::fulgur::{Fulgur, utils::updater::check_for_updates};
use gpui_component::{
    WindowExt,
    button::{Button, ButtonVariants},
    notification::{Notification, NotificationType},
};

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
    ]
);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct SwitchTheme(pub SharedString);

#[derive(Action, Clone, PartialEq)]
#[action(namespace = fulgur, no_json)]
pub struct OpenRecentFile(pub PathBuf);

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
        let mut items = Vec::new();
        items.push(MenuItem::action("No recent files", NoneAction));
        items
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

impl Fulgur {
    /// Check for updates, open the download page in the browser if an update is available, update the menus to show the update available action and show notifications
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn check_for_updates(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(update_link) = self.shared_state(cx).update_link.lock().as_ref() {
            match open::that(update_link) {
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
                    if let Some(update_info) = update_info {
                        let _ = view.update(cx, |this, cx| {
                            {
                                let mut update_link = this.shared_state(cx).update_link.lock();
                                *update_link = Some(update_info.download_url.clone());
                            }
                            let menus = build_menus(
                                &this.settings.recent_files.get_files(),
                                this.shared_state(cx).update_link.lock().clone(),
                            );
                            cx.set_menus(menus);
                            cx.notify();
                        });
                        let notification_text = SharedString::from(format!(
                            "Update found! {} -> {}",
                            update_info.current_version, update_info.latest_version
                        ));
                        let update_info_clone = update_info.clone();
                        let notification = Notification::new().message(notification_text).action(
                            move |_, _, cx| {
                                let _download_url = update_info_clone.download_url.clone();
                                Button::new("download")
                                    .primary()
                                    .label("Download")
                                    .mr_2()
                                    .on_click(cx.listener({
                                        let url = update_info.download_url.clone();
                                        move |this, _, window, cx| {
                                            match open::that(&url) {
                                                Ok(_) => {
                                                    log::debug!("Successfully opened browser");
                                                }
                                                Err(e) => {
                                                    log::error!("Failed to open browser: {}", e);
                                                }
                                            }
                                            this.dismiss(window, cx);
                                        }
                                    }))
                            },
                        );
                        window.push_notification(notification, cx);
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
