use super::app_menus::build_menus;
use crate::fulgur::{
    Fulgur,
    utils::updater::{check_for_updates, is_valid_release_page_url},
};
use gpui::{Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};

impl Fulgur {
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
