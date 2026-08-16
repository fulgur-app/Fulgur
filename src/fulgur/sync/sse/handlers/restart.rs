use crate::fulgur::shared_state::SyncState;
use crate::fulgur::ui::notifications::progress::{CancelCallback, spawn_with_progress};
use crate::fulgur::utils::worker::{Worker, WorkerHooks, dispose_off_thread};
use crate::fulgur::{
    Fulgur,
    settings::ServerProfile,
    sync::synchronization::{
        SynchronizationError, SynchronizationStatus, apply_initial_sync_outcome,
        initial_synchronization, set_sync_server_connection_status,
    },
};
use futures::channel::mpsc::UnboundedSender;
use gpui::{Context, SharedString, Window};
use gpui_component::notification::NotificationType;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

use super::super::{
    connection::{SseAgents, SseShareState, connect_sse},
    types::{SSE_WORKER_JOIN_TIMEOUT, SseEvent},
};

/// Delay left between dropping the previous SSE worker and opening the new
/// stream, so the server sees the old connection close first.
const SSE_RECONNECT_DELAY: Duration = Duration::from_millis(200);

/// Everything a background SSE restart needs to retire the previous worker and
/// bring a fresh connection up.
struct SseRestartJob {
    profile: ServerProfile,
    old_worker: Option<Worker>,
    sse_tx: UnboundedSender<SseEvent>,
    new_worker_hooks: WorkerHooks,
    sync_state: Arc<SyncState>,
    http_agent: Arc<ureq::Agent>,
    sse_http_agent: Arc<ureq::Agent>,
}

/// Result of running an [`SseRestartJob`].
enum SseRestartOutcome {
    /// Initial synchronization succeeded and a new SSE connection was started.
    Completed {
        device_name: String,
        notifications: Vec<(NotificationType, SharedString)>,
    },
    /// Initial synchronization failed, so no SSE connection was started.
    Failed(SynchronizationError),
    /// The user cancelled before the outcome could be published.
    Cancelled,
}

impl SseRestartJob {
    /// Retire the previous worker, re-run the initial synchronization, and opena new SSE connection.
    ///
    /// ### Arguments
    /// - `cancel_flag`: Cancel flag of the progress indicator, when the caller
    ///   shows one. Checked once before any work with a visible effect starts.
    ///
    /// ### Returns
    /// - `SseRestartOutcome`: What the caller should report to the user.
    fn run(mut self, cancel_flag: Option<&AtomicBool>) -> SseRestartOutcome {
        drop(self.old_worker.take());
        thread::sleep(SSE_RECONNECT_DELAY);
        if cancel_flag.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return SseRestartOutcome::Cancelled;
        }
        match initial_synchronization(
            &self.profile,
            &self.sync_state.token_state,
            &self.http_agent,
            &self.sync_state.pending_ack_share_ids,
        ) {
            Ok(outcome) => {
                let (device_name, notifications) =
                    apply_initial_sync_outcome(&self.sync_state, &self.profile.name, outcome);
                log::info!(
                    "Profile '{}': initial sync succeeded, starting new SSE",
                    self.profile.name
                );
                self.start_sse();
                SseRestartOutcome::Completed {
                    device_name,
                    notifications,
                }
            }
            Err(e) => {
                log::error!(
                    "Profile '{}': initial sync failed, not starting SSE: {e}",
                    self.profile.name
                );
                SseRestartOutcome::Failed(e)
            }
        }
    }

    /// Hand the profile's shared state to a freshly spawned SSE worker.
    fn start_sse(self) {
        let agents = SseAgents {
            rest: Arc::clone(&self.http_agent),
            stream: Arc::clone(&self.sse_http_agent),
        };
        let share_state = SseShareState {
            pending_shared_files: Arc::clone(&self.sync_state.pending_shared_files),
            pending_ack_share_ids: Arc::clone(&self.sync_state.pending_ack_share_ids),
            max_file_size_bytes: Arc::clone(&self.sync_state.max_file_size_bytes),
            server_version: Arc::clone(&self.sync_state.server_version),
        };
        if let Err(e) = connect_sse(
            &self.profile,
            self.sse_tx,
            &self.new_worker_hooks,
            self.sync_state.connection_status.clone(),
            &self.sync_state.token_state,
            &agents,
            &share_state,
        ) {
            log::error!("Profile '{}': failed to start SSE: {e}", self.profile.name);
        }
    }
}

impl Fulgur {
    /// Signal the old SSE worker, rotate a fresh `Worker` into the shared SSE
    /// state, and validate that the profile is ready to connect.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile to restart.
    /// - `cx`: The Fulgur context.
    ///
    /// ### Returns
    /// - `Some(SseRestartJob)`: A job ready for the caller to run on a thread.
    /// - `None`: The profile was not found, is inactive, or the master switch is off.
    fn prepare_sse_restart(
        &mut self,
        profile_id: &str,
        cx: &mut Context<Self>,
    ) -> Option<SseRestartJob> {
        let Some(profile) = self
            .settings
            .app_settings
            .synchronization_settings
            .find_profile(profile_id)
            .cloned()
        else {
            log::warn!("prepare_sse_restart: profile id '{profile_id}' not found in settings");
            return None;
        };
        let master_on = self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated;
        if !master_on || !profile.is_active {
            log::info!(
                "Profile '{}' not active or master switch off, SSE connection not started",
                profile.name
            );
            return None;
        }

        // Ensure the app-scope consumer task for this profile's events is running.
        Self::spawn_sse_event_consumer(profile_id, cx);
        let shared = Fulgur::shared_state(cx);
        let sync_state = shared.sync_state_for(profile_id);
        let http_agent = Arc::clone(&shared.http_agent);
        let sse_http_agent = Arc::clone(&shared.sse_http_agent);
        let (sse_tx, new_worker_hooks, old_worker) = {
            let mut sse = sync_state.sse.lock();
            let old_worker = sse.worker.take();
            if let Some(ref worker) = old_worker {
                log::info!("Profile '{profile_id}': signaling SSE shutdown");
                worker.signal_shutdown();
            }
            let sse_tx = sse
                .sse_event_tx
                .clone()
                .expect("shared SSE state must own a live event sender");
            let new_worker =
                Worker::new(format!("fulgur-sse-{profile_id}"), SSE_WORKER_JOIN_TIMEOUT);
            let new_worker_hooks = new_worker.hooks();
            sse.worker = Some(new_worker);
            (sse_tx, new_worker_hooks, old_worker)
        };

        Some(SseRestartJob {
            profile,
            old_worker,
            sse_tx,
            new_worker_hooks,
            sync_state,
            http_agent,
            sse_http_agent,
        })
    }

    /// Stop the SSE connection of a profile without starting a new one.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose SSE worker should be stopped.
    /// - `cx`: The context of the application.
    pub fn stop_sse_connection_for(&self, profile_id: &str, cx: &mut Context<Self>) {
        let sync_state = Fulgur::shared_state(cx).sync_state_for(profile_id);
        let worker = sync_state.sse.lock().worker.take();
        if let Some(worker) = worker {
            log::info!("Profile '{profile_id}': stopping SSE connection");
            dispose_off_thread(worker, cx);
        }
        set_sync_server_connection_status(
            &sync_state.connection_status,
            SynchronizationStatus::NotActivated,
        );
        *sync_state.connecting_since.lock() = None;
        cx.notify();
    }

    /// Restart the SSE connection for a single profile.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose SSE worker should be restarted.
    /// - `cx`: The context of the application.
    pub fn restart_sse_connection_for(&mut self, profile_id: &str, cx: &mut Context<Self>) {
        let Some(job) = self.prepare_sse_restart(profile_id, cx) else {
            return;
        };
        thread::spawn(move || {
            job.run(None);
        });
    }

    /// Restart the SSE connection for a single profile, showing a progress
    /// indicator and a success/error notification when the connection attempt
    /// completes.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose SSE worker should be restarted.
    /// - `window`: The window to attach the progress indicator to.
    /// - `cx`: The context of the application.
    pub fn restart_sse_connection_for_with_progress(
        &mut self,
        profile_id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(job) = self.prepare_sse_restart(profile_id, cx) else {
            return;
        };
        let sync_state = Arc::clone(&job.sync_state);
        let notification_tx = sync_state.notification_tx.clone();
        let profile_name = job.profile.name.clone();

        set_sync_server_connection_status(
            &sync_state.connection_status,
            SynchronizationStatus::Connecting,
        );
        *sync_state.connecting_since.lock() = Some(Instant::now());

        let cancel_state = Arc::clone(&sync_state);
        let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, _cx| {
            set_sync_server_connection_status(
                &cancel_state.connection_status,
                SynchronizationStatus::Disconnected,
            );
            *cancel_state.connecting_since.lock() = None;
        }));

        spawn_with_progress(
            window,
            cx,
            format!("Connecting to {profile_name}...").into(),
            cancel_callback,
            move |cancel_flag| {
                let (notification, status) = match job.run(Some(cancel_flag)) {
                    SseRestartOutcome::Cancelled => return,
                    SseRestartOutcome::Completed {
                        device_name,
                        notifications,
                    } => {
                        let notification = notifications.into_iter().next().unwrap_or_else(|| {
                            (
                                NotificationType::Success,
                                SharedString::from(format!(
                                    "{profile_name}: Connection successful as {device_name}"
                                )),
                            )
                        });
                        (notification, SynchronizationStatus::Connected)
                    }
                    SseRestartOutcome::Failed(e) => (
                        (
                            NotificationType::Error,
                            SharedString::from(format!("{profile_name}: Connection failed: {e}")),
                        ),
                        SynchronizationStatus::from_error(&e),
                    ),
                };
                set_sync_server_connection_status(&sync_state.connection_status, status);
                *sync_state.connecting_since.lock() = None;
                let status_tx = sync_state.sse.lock().sse_event_tx.clone();
                if let Some(tx) = status_tx {
                    let _ = tx.unbounded_send(SseEvent::ConnectionStatusChanged);
                }
                let _ = notification_tx.unbounded_send(notification);
            },
        );
    }
}
