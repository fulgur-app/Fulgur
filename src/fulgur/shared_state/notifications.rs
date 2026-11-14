use super::SharedAppState;
use futures::StreamExt;
use gpui_kit::component::WindowExt;
use gpui_kit::component::notification::{Notification, NotificationDelivery, NotificationType};
use gpui_kit::{App, AsyncApp, SharedString};
use std::time::Duration;

/// Where a notification came from, which decides whether it is worth surfacing outside the app.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum NotificationOrigin {
    /// The user triggered the action and is looking at the window, so the in-app toast suffices.
    #[default]
    Foreground,
    /// Background work the user did not just ask for: sync, shares, SSE, startup restore.
    Background,
}

/// A user-facing notification: severity, message, and where it came from.
#[derive(Debug, Clone)]
pub struct AppNotification {
    pub notification_type: NotificationType,
    pub message: SharedString,
    pub origin: NotificationOrigin,
}

impl AppNotification {
    /// Build a notification for work the user just triggered, shown as an in-app toast only.
    ///
    /// ### Arguments
    /// - `notification_type`: The severity of the notification.
    /// - `message`: The text to display.
    ///
    /// ### Returns
    /// - `AppNotification`: The notification, tagged [`NotificationOrigin::Foreground`]
    pub fn foreground(
        notification_type: NotificationType,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            notification_type,
            message: message.into(),
            origin: NotificationOrigin::Foreground,
        }
    }

    /// Build a notification for background work, eligible for the OS notification center.
    ///
    /// ### Arguments
    /// - `notification_type`: The severity of the notification.
    /// - `message`: The text to display.
    ///
    /// ### Returns
    /// - `AppNotification`: The notification, tagged [`NotificationOrigin::Background`]
    pub fn background(
        notification_type: NotificationType,
        message: impl Into<SharedString>,
    ) -> Self {
        Self {
            notification_type,
            message: message.into(),
            origin: NotificationOrigin::Background,
        }
    }
}

/// Maximum delivery attempts before a notification with no window to show it in is dropped.
const NOTIFICATION_DELIVERY_MAX_ATTEMPTS: usize = 60;

/// Delay between notification delivery attempts while no window is available.
const NOTIFICATION_DELIVERY_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Spawn the app-scope task that delivers queued notifications to the focused window.
///
/// ### Arguments
/// - `cx`: The application context.
pub fn spawn_notification_consumer(cx: &mut App) {
    let Some(mut rx) = cx.global::<SharedAppState>().notification_rx.lock().take() else {
        return;
    };
    cx.spawn(async move |cx| {
        while let Some(notification) = rx.next().await {
            deliver_notification(notification, cx).await;
        }
    })
    .detach();
}

/// Deliver one notification to the focused window, retrying while no window exists.
///
/// ### Arguments
/// - `notification`: The notification to deliver.
/// - `cx`: The async application context.
async fn deliver_notification(notification: AppNotification, cx: &mut AsyncApp) {
    for attempt in 0..NOTIFICATION_DELIVERY_MAX_ATTEMPTS {
        if attempt > 0 {
            cx.background_executor()
                .timer(NOTIFICATION_DELIVERY_RETRY_DELAY)
                .await;
        }
        if cx.update(|cx| push_notification_to_focused_window(notification.clone(), cx)) {
            return;
        }
    }
    log::warn!(
        "No window available to display notification, dropping: {}",
        notification.message
    );
}

/// Push a notification to the focused window, falling back to any open window.
///
/// ### Arguments
/// - `notification`: The notification to display.
/// - `cx`: The application context.
///
/// ### Returns
/// - `true`: The notification was pushed to a window.
/// - `false`: No window is currently available.
fn push_notification_to_focused_window(notification: AppNotification, cx: &mut App) -> bool {
    let focused_window_id = cx
        .try_global::<crate::fulgur::window_manager::WindowManager>()
        .and_then(crate::fulgur::window_manager::WindowManager::get_last_focused);
    let windows = cx.windows();
    let handle = focused_window_id
        .and_then(|id| windows.iter().find(|w| w.window_id() == id).copied())
        .or_else(|| windows.first().copied());
    let Some(handle) = handle else {
        return false;
    };
    let delivery = notification_delivery(&notification, cx);
    handle
        .update(cx, |_, window, cx| {
            window.push_notification(
                Notification::new()
                    .message(notification.message)
                    .with_type(notification.notification_type)
                    .delivery(delivery),
                cx,
            );
        })
        .is_ok()
}

/// Decide where a queued notification is presented.
///
/// ### Arguments
/// - `notification`: The notification about to be delivered.
/// - `cx`: The application context, used to read the current setting.
///
/// ### Returns
/// - `NotificationDelivery::InAppAndSystem`: The notification is background work and the user has
///   left system notifications enabled
/// - `NotificationDelivery::InApp`: Every other case
fn notification_delivery(notification: &AppNotification, cx: &App) -> NotificationDelivery {
    let system_enabled = cx
        .try_global::<SharedAppState>()
        .is_some_and(|shared| shared.settings.app_settings.system_notifications);
    if system_enabled && notification.origin == NotificationOrigin::Background {
        NotificationDelivery::InAppAndSystem
    } else {
        NotificationDelivery::InApp
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::*;
    use crate::fulgur::settings::Settings;
    use crate::fulgur::state::StateDb;
    use gpui_kit::TestAppContext;
    use parking_lot::Mutex;
    use std::sync::Arc;

    /// Install a `SharedAppState` whose `system_notifications` setting has the given value.
    ///
    /// ### Arguments
    /// - `cx`: The GPUI test application context.
    /// - `system_notifications`: The value to store in the application settings.
    fn set_up_shared_state(cx: &mut TestAppContext, system_notifications: bool) {
        cx.update(|cx| {
            let mut settings = Settings::new();
            settings.app_settings.system_notifications = system_notifications;
            let state_db = StateDb::open_in_memory().expect("failed to open state database");
            cx.set_global(SharedAppState::new(
                settings,
                Arc::new(Mutex::new(Vec::new())),
                None,
                Some(state_db),
            ));
        });
    }

    #[gpui_kit::test]
    fn test_background_notification_reaches_system_when_enabled(cx: &mut TestAppContext) {
        set_up_shared_state(cx, true);
        let notification = AppNotification::background(NotificationType::Info, "share received");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(delivery, NotificationDelivery::InAppAndSystem);
    }

    #[gpui_kit::test]
    fn test_background_notification_stays_in_app_when_disabled(cx: &mut TestAppContext) {
        set_up_shared_state(cx, false);
        let notification = AppNotification::background(NotificationType::Info, "share received");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(delivery, NotificationDelivery::InApp);
    }

    #[gpui_kit::test]
    fn test_foreground_notification_never_reaches_system(cx: &mut TestAppContext) {
        set_up_shared_state(cx, true);
        let notification =
            AppNotification::foreground(NotificationType::Error, "failed to save settings");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(
            delivery,
            NotificationDelivery::InApp,
            "a notification the user just triggered is already on screen"
        );
    }

    #[gpui_kit::test]
    fn test_delivery_without_shared_state_stays_in_app(cx: &mut TestAppContext) {
        let notification = AppNotification::background(NotificationType::Info, "early failure");
        let delivery = cx.update(|cx| notification_delivery(&notification, cx));
        assert_eq!(delivery, NotificationDelivery::InApp);
    }
}
