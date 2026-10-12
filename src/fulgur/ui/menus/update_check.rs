use crate::fulgur::{
    Fulgur,
    shared_state::SharedAppState,
    ui::notifications::update_notification::make_update_notification,
    utils::updater::{UpdateInfo, check_for_updates, is_valid_release_page_url},
    window_manager::WindowManager,
};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{App, BorrowAppContext, Context, SharedString, Window};
use std::time::Duration;

/// Delay before the startup update check, so it does not compete with window restoration
const STARTUP_UPDATE_CHECK_DELAY: Duration = Duration::from_secs(5);

impl Fulgur {
    /// Open the release page when an update is already known, otherwise check for updates and report the outcome
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub fn check_for_updates(&self, window: &mut Window, cx: &mut Context<Self>) {
        if let Some(update_info) = &Fulgur::shared_state(cx).available_update {
            open_release_page(&update_info.download_url);
            return;
        }
        let bg = cx.background_executor().clone();
        cx.spawn_in(window, async move |view, window| {
            log::debug!("Checking for updates");
            let result = bg
                .spawn(async move { check_for_updates(env!("CARGO_PKG_VERSION")) })
                .await;
            window
                .update(|window, cx| match result {
                    Ok(Some(update_info)) => {
                        let _ = view.update(cx, |this, cx| {
                            this.record_available_update(update_info, window, cx);
                        });
                    }
                    Ok(None) => {
                        let notification = SharedString::from("No update found");
                        window.push_notification((NotificationType::Info, notification), cx);
                    }
                    Err(e) => {
                        log::warn!("Failed to check for updates: {e}");
                        let notification =
                            SharedString::from(format!("Could not check for updates: {e}"));
                        window.push_notification((NotificationType::Error, notification), cx);
                    }
                })
                .ok();
        })
        .detach();
    }

    /// Check for updates once, shortly after startup, and announce a newer release in the focused window
    ///
    /// ### Arguments
    /// - `cx`: The application context
    pub fn spawn_startup_update_check(cx: &mut App) {
        cx.spawn(async move |cx| {
            cx.background_executor()
                .timer(STARTUP_UPDATE_CHECK_DELAY)
                .await;
            log::info!("Checking for updates...");
            let result = cx
                .background_executor()
                .spawn(async move { check_for_updates(env!("CARGO_PKG_VERSION")) })
                .await;
            match result {
                Ok(Some(update_info)) => {
                    cx.update(|cx| Self::announce_available_update(update_info, cx));
                }
                Ok(None) => {}
                Err(e) => log::warn!("Failed to check for updates: {e}"),
            }
        })
        .detach();
    }

    /// Announce a newer release in the target window, or only record it when no window is open
    ///
    /// ### Arguments
    /// - `update_info`: The newer release found by the update check
    /// - `cx`: The application context
    fn announce_available_update(update_info: UpdateInfo, cx: &mut App) {
        let Some((handle, fulgur)) = WindowManager::target_window(cx) else {
            cx.update_global::<SharedAppState, _>(|shared, _| {
                shared.available_update = Some(update_info);
            });
            return;
        };
        if let Err(e) = handle.update(cx, |_, window, cx| {
            fulgur.update(cx, |this, cx| {
                this.record_available_update(update_info, window, cx);
            });
        }) {
            log::error!("Failed to announce the available update: {e}");
        }
    }

    /// Record a newer release as the lasting update state, show its toast and switch the menu to "Update available"
    ///
    /// ### Arguments
    /// - `update_info`: The newer release found by the update check
    /// - `window`: The window showing the toast
    /// - `cx`: The application context
    pub(crate) fn record_available_update(
        &mut self,
        update_info: UpdateInfo,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        log::info!(
            "Update available: {} -> {}",
            update_info.current_version,
            update_info.latest_version
        );
        window.push_notification(make_update_notification(&update_info), cx);
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.available_update = Some(update_info);
        });
        self.rebuild_menus(cx);
    }
}

/// Open the release page in the browser, refusing any URL outside the project's releases
///
/// ### Arguments
/// - `url`: The release page URL reported by the update check
fn open_release_page(url: &str) {
    if !is_valid_release_page_url(url) {
        log::error!("Refusing to open non-canonical update URL: {url}");
        return;
    }
    match open::that(url) {
        Ok(()) => log::debug!("Successfully opened browser"),
        Err(e) => log::error!("Failed to open browser: {e}"),
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{Fulgur, shared_state::SharedAppState, utils::updater::UpdateInfo};
    use crate::test_support::setup_fulgur_with_root;
    use gpui_kit::{App, OwnedMenuItem, TestAppContext};

    /// Build a newer release as the update check would report it.
    ///
    /// ### Returns
    /// - `UpdateInfo`: A release newer than the running version
    fn newer_release() -> UpdateInfo {
        UpdateInfo {
            current_version: "0.13.0".to_string(),
            latest_version: "99.0.0".to_string(),
            is_newer: true,
            download_url: "https://github.com/fulgur-app/Fulgur/releases/tag/v99.0.0".to_string(),
            release_notes: String::new(),
        }
    }

    /// Name of the update entry currently shown in the application menu.
    ///
    /// ### Arguments
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `Some(String)`: The label of the update entry
    /// - `None`: The menus are unset or have no update entry
    fn update_menu_entry(cx: &App) -> Option<String> {
        cx.get_menus()?
            .into_iter()
            .flat_map(|menu| menu.items)
            .find_map(|item| match item {
                OwnedMenuItem::Action { name, .. }
                    if name == "Update available" || name == "Check for updates" =>
                {
                    Some(name)
                }
                _ => None,
            })
    }

    #[gpui_kit::test]
    fn test_available_update_survives_renders(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.record_available_update(newer_release(), window, cx);
            });
        });
        visual_cx.run_until_parked();
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        let latest_version = visual_cx.update(|_, cx| {
            cx.global::<SharedAppState>()
                .available_update
                .as_ref()
                .map(|update| update.latest_version.clone())
        });
        assert_eq!(latest_version.as_deref(), Some("99.0.0"));
    }

    #[gpui_kit::test]
    fn test_menu_rebuild_keeps_update_entry(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        visual_cx.update(|_, cx| fulgur.update(cx, Fulgur::rebuild_menus));
        assert_eq!(
            visual_cx.update(|_, cx| update_menu_entry(cx)).as_deref(),
            Some("Check for updates")
        );
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.record_available_update(newer_release(), window, cx);
            });
        });
        visual_cx.run_until_parked();
        visual_cx.update(|window, cx| window.draw(cx).clear(cx));
        visual_cx.update(|_, cx| fulgur.update(cx, Fulgur::rebuild_menus));
        assert_eq!(
            visual_cx.update(|_, cx| update_menu_entry(cx)).as_deref(),
            Some("Update available"),
            "a later menu rebuild (file open, theme switch) must keep the update entry"
        );
    }
}
