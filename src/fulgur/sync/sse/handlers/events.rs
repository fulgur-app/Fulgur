use crate::fulgur::{
    Fulgur, settings::ProfileId, sync::synchronization::SynchronizationStatus,
    utils::utilities::collect_events,
};
use gpui::{App, Context, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
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

    /// Handle a single SSE event for a specific profile on the UI thread.
    ///
    /// Runs on the main/UI thread (it needs `window` and `cx`) and only performs UI-facing reactions.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile that produced the event.
    /// - `event`: The SSE event to handle.
    /// - `window`: The window to show notifications in.
    /// - `cx`: The application context.
    pub fn handle_sse_event_for(
        &mut self,
        profile_id: &ProfileId,
        event: SseEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let now = Instant::now();
        let state = self.sse_states.entry(profile_id.clone()).or_default();
        if let Some(last_time) = state.last_sse_event
            && now.duration_since(last_time) < Duration::from_millis(500)
        {
            return;
        }
        state.last_sse_event = Some(now);
        let sync_state = Fulgur::shared_state(cx).sync_state_for(profile_id);
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
                }
            }
            SseEvent::ShareAvailable(notification) => {
                log::debug!(
                    "Share doorbell on UI tick (share_id={})",
                    notification.share_id
                );
                let message = SharedString::from("New file received".to_string());
                window.push_notification((NotificationType::Info, message), cx);
            }
            SseEvent::Error(err) => {
                log::error!("SSE error for profile '{profile_id}': {err}");
            }
        }
    }

    /// Handle an SSE event using the primary profile, on the UI thread. Performs UI-facing reactions and never downloads shares.
    ///
    /// ### Arguments
    /// - `event`: The SSE event to handle.
    /// - `window`: The window to show notifications in.
    /// - `cx`: The application context.
    pub fn handle_sse_event(
        &mut self,
        event: SseEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let primary_id = self
            .settings
            .app_settings
            .synchronization_settings
            .primary_profile()
            .map(|p| p.id.clone())
            .unwrap_or_default();
        self.handle_sse_event_for(&primary_id, event, window, cx);
    }

    /// Drain pending SSE events from every profile's channel and dispatch them on the UI thread.
    ///
    /// ### Arguments
    /// - `window`: The window to handle events in.
    /// - `cx`: The application context.
    pub fn process_sse_events(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let profile_ids: Vec<ProfileId> = self.sse_states.keys().cloned().collect();
        for profile_id in profile_ids {
            let events = self
                .sse_states
                .get(&profile_id)
                .map(|s| collect_events(&s.sse_events))
                .unwrap_or_default();
            for event in events {
                self.handle_sse_event_for(&profile_id, event, window, cx);
            }
        }
    }
}
