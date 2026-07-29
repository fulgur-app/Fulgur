use crate::fulgur::Fulgur;
use gpui::{Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::path::Path;

impl Fulgur {
    /// Show notification when file is reloaded
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was reloaded
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_reloaded(
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File {filename} has been updated externally"));
        window.push_notification((NotificationType::Info, message), cx);
    }

    /// Show notification when file is deleted
    ///
    /// ### Arguments
    /// - `path`: The path to the file that was deleted
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_deleted(
        path: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File '{filename}' deleted externally"));
        window.push_notification((NotificationType::Warning, message), cx);
    }

    /// Show notification when file is renamed
    ///
    /// ### Arguments
    /// - `from`: The path to the file that was renamed from
    /// - `to`: The path to the file that was renamed to
    /// - `window`: The window to show the notification in
    /// - `cx`: The application context
    pub(crate) fn show_notification_file_renamed(
        from: &Path,
        to: &Path,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let old_name = from.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let new_name = to.file_name().and_then(|n| n.to_str()).unwrap_or("file");
        let message = SharedString::from(format!("File renamed: {old_name} → {new_name}"));
        window.push_notification((NotificationType::Info, message), cx);
    }
}
