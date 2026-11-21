use crate::fulgur::{Fulgur, utils::paths};
use gpui_kit::component::{WindowExt, notification::NotificationType};
use gpui_kit::{Context, SharedString, Window};

impl Fulgur {
    /// Open Fulgur's configuration folder (settings, session state, logs, themes) in the OS file manager.
    ///
    /// ### Arguments
    /// - `window`: The active window
    /// - `cx`: The application context
    pub fn open_config_folder(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let result = paths::config_dir().and_then(|folder| {
            open::that_detached(&folder)
                .map_err(|e| anyhow::anyhow!("cannot open {}: {}", folder.display(), e))
        });
        if let Err(e) = result {
            log::error!("Failed to open the configuration folder: {e}");
            window.push_notification(
                (
                    NotificationType::Error,
                    SharedString::from("The configuration folder could not be opened."),
                ),
                cx,
            );
        }
    }
}
