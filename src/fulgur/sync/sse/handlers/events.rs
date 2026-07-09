use crate::fulgur::{
    Fulgur, settings::ProfileId, sync::synchronization::SynchronizationStatus,
    window_manager::WindowManager,
};
use futures::StreamExt;
use gpui::{App, SharedString};
use gpui_component::notification::NotificationType;
use std::time::{Duration, Instant};

use super::super::types::SseEvent;

impl Fulgur {
    /// Check if any profile is currently connected to its sync server.
    ///
    /// ### Returns
    /// - `true`: At least one profile reports a connected status.
    /// - `false`: No profile is connected.
    pub fn is_connected(&self, cx: &App) -> bool {
        let shared = Fulgur::shared_state(cx);
        let states = shared.sync_states.read();
        states
            .values()
            .any(|s| s.connection_status.lock().is_connected())
    }

    /// Spawn the app-scope task that consumes one profile's SSE events.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose events should be consumed.
    /// - `cx`: The application context.
    pub fn spawn_sse_event_consumer(profile_id: &str, cx: &mut App) {
        let sync_state = Fulgur::shared_state(cx).sync_state_for(profile_id);
        let Some(mut events) = sync_state.sse.lock().sse_events.take() else {
            return;
        };
        let profile_id: ProfileId = profile_id.to_string();
        cx.spawn(async move |cx| {
            while let Some(event) = events.next().await {
                cx.update(|cx| Self::handle_sse_event_for_profile(&profile_id, event, cx));
            }
        })
        .detach();
    }

    /// Handle a single SSE event for a specific profile on the UI thread.
    ///
    /// Runs at application scope: state updates go to the profile's shared sync
    /// state, user-facing notifications go through the app notification channel,
    /// and windows are re-rendered when the event changed something they display.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile that produced the event.
    /// - `event`: The SSE event to handle.
    /// - `cx`: The application context.
    pub fn handle_sse_event_for_profile(profile_id: &ProfileId, event: SseEvent, cx: &mut App) {
        let now = Instant::now();
        let sync_state = Fulgur::shared_state(cx).sync_state_for(profile_id);
        {
            let mut sse = sync_state.sse.lock();
            if let Some(last_time) = sse.last_sse_event
                && now.duration_since(last_time) < Duration::from_millis(500)
            {
                return;
            }
            sse.last_sse_event = Some(now);
        }
        match event {
            SseEvent::Heartbeat { timestamp } => {
                log::debug!("SSE heartbeat received for profile '{profile_id}': {timestamp}");
                let was_disconnected = !sync_state.connection_status.lock().is_connected();
                *sync_state.last_heartbeat.lock() = Some(now);
                if was_disconnected {
                    *sync_state.connection_status.lock() = SynchronizationStatus::Connected;
                    log::info!(
                        "Profile '{profile_id}': connection restored on heartbeat after timeout"
                    );
                    Self::notify_all_windows(cx);
                }
            }
            SseEvent::ShareAvailable(notification) => {
                log::debug!(
                    "Share doorbell received on consumer task (share_id={})",
                    notification.share_id
                );
                Fulgur::shared_state(cx).notify((
                    NotificationType::Info,
                    SharedString::from("New file received"),
                ));
                // Wake every window so the queued share is decrypted and opened.
                Self::notify_all_windows(cx);
            }
            SseEvent::Error(err) => {
                log::error!("SSE error for profile '{profile_id}': {err}");
            }
        }
    }

    /// Request a re-render of every open window.
    ///
    /// ### Arguments
    /// - `cx`: The application context.
    fn notify_all_windows(cx: &mut App) {
        let windows = match cx.try_global::<WindowManager>() {
            Some(manager) => manager.get_all_windows(),
            None => return,
        };
        for weak in windows {
            if let Some(entity) = weak.upgrade() {
                entity.update(cx, |_, cx| cx.notify());
            }
        }
    }
}
