use crate::fulgur::shared_state::AppNotification;
use crate::fulgur::shared_state::SyncState;
use crate::fulgur::ui::notifications::progress::{CancelCallback, spawn_with_progress};
use crate::fulgur::utils::worker::{Worker, WorkerHooks, dispose_off_thread};
use crate::fulgur::{
    Fulgur,
    settings::ServerProfile,
    sync::synchronization::{
        SynchronizationError, SynchronizationStatus, set_sync_server_connection_status,
    },
};
use futures::channel::mpsc::UnboundedSender;
use gpui_kit::component::notification::NotificationType;
use gpui_kit::{Context, Window};
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use super::super::{
    connection::{InitialSyncReport, InitialSyncStage, SseAgents, connect_sse_after_initial_sync},
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
    /// A new SSE worker is running; it synchronizes, retrying transient
    /// failures, then opens its stream and publishes each outcome itself.
    Started,
    /// No SSE worker could be started.
    Failed(SynchronizationError),
    /// The user cancelled before the new worker was started.
    Cancelled,
}

impl SseRestartJob {
    /// Retire the previous worker and start a new one that re-runs the initial
    /// synchronization before opening its SSE connection.
    ///
    /// ### Arguments
    /// - `cancel_flag`: Cancel flag of the progress indicator, when the caller
    ///   shows one. Checked once before any work with a visible effect starts.
    /// - `first_attempt_tx`: Receives the outcome of the new worker's first
    ///   synchronization attempt, when the caller reports it to the user.
    ///
    /// ### Returns
    /// - `SseRestartOutcome`: Whether the new worker is running.
    fn run(
        mut self,
        cancel_flag: Option<&AtomicBool>,
        first_attempt_tx: Option<mpsc::Sender<InitialSyncReport>>,
    ) -> SseRestartOutcome {
        drop(self.old_worker.take());
        thread::sleep(SSE_RECONNECT_DELAY);
        if cancel_flag.is_some_and(|flag| flag.load(Ordering::Acquire)) {
            return SseRestartOutcome::Cancelled;
        }
        let profile_name = self.profile.name.clone();
        match self.start_sse(first_attempt_tx) {
            Ok(()) => SseRestartOutcome::Started,
            Err(e) => {
                log::error!("Profile '{profile_name}': failed to start SSE: {e}");
                SseRestartOutcome::Failed(e)
            }
        }
    }

    /// Hand the profile's shared state to a freshly spawned SSE worker.
    ///
    /// ### Arguments
    /// - `first_attempt_tx`: Receives the outcome of the worker's first
    ///   synchronization attempt.
    ///
    /// ### Errors
    /// - Returns a `SynchronizationError` when the profile has no server URL
    ///   or the SSE worker thread could not be started.
    ///
    /// ### Returns
    /// - `Ok(())`: The SSE worker is running and publishes each synchronization outcome
    /// - `Err(SynchronizationError)`: No SSE worker was started
    fn start_sse(
        self,
        first_attempt_tx: Option<mpsc::Sender<InitialSyncReport>>,
    ) -> Result<(), SynchronizationError> {
        let agents = SseAgents {
            rest: Arc::clone(&self.http_agent),
            stream: Arc::clone(&self.sse_http_agent),
        };
        let initial_sync = InitialSyncStage {
            sync_state: Arc::clone(&self.sync_state),
            first_attempt_tx,
        };
        connect_sse_after_initial_sync(
            &self.profile,
            self.sse_tx,
            &self.new_worker_hooks,
            &agents,
            initial_sync,
        )
    }
}

/// Polling interval of the progress indicator waiting for the first attempt.
const FIRST_ATTEMPT_POLL: Duration = Duration::from_millis(100);

/// Wait for a new worker's first synchronization attempt to finish.
///
/// ### Arguments
/// - `report_rx`: Receiver the worker sends its first attempt's outcome to.
/// - `cancel_flag`: Cancel flag of the progress indicator.
///
/// ### Returns
/// - `Some(InitialSyncReport)`: The first attempt's outcome.
/// - `None`: The user cancelled, or the worker stopped before reporting.
fn wait_for_first_attempt(
    report_rx: &mpsc::Receiver<InitialSyncReport>,
    cancel_flag: &AtomicBool,
) -> Option<InitialSyncReport> {
    loop {
        if cancel_flag.load(Ordering::Acquire) {
            return None;
        }
        match report_rx.recv_timeout(FIRST_ATTEMPT_POLL) {
            Ok(report) => return Some(report),
            Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => return None,
        }
    }
}

/// Build the notification reporting a first synchronization attempt.
///
/// ### Arguments
/// - `profile_name`: Profile name used in the notification text.
/// - `report`: The first attempt's outcome.
///
/// ### Returns
/// - `AppNotification`: A version warning when the server warrants one, the
///   success message otherwise, or the failure with its retry state.
fn first_attempt_notification(profile_name: &str, report: InitialSyncReport) -> AppNotification {
    match report {
        InitialSyncReport::Synced {
            device_name,
            notifications,
        } => notifications.into_iter().next().unwrap_or_else(|| {
            AppNotification::background(
                NotificationType::Success,
                format!("{profile_name}: Connection successful as {device_name}"),
            )
        }),
        InitialSyncReport::Failed { error, retrying } => {
            let retry_note = if retrying {
                " Retrying in the background."
            } else {
                ""
            };
            AppNotification::background(
                NotificationType::Error,
                format!("{profile_name}: Connection failed: {error}.{retry_note}"),
            )
        }
    }
}

/// Flag a profile as connecting, which starts the status bar's spinner delay.
///
/// ### Arguments
/// - `sync_state`: The profile's shared sync state.
fn mark_connecting(sync_state: &SyncState) {
    set_sync_server_connection_status(
        &sync_state.connection_status,
        SynchronizationStatus::Connecting,
    );
    *sync_state.connecting_since.lock() = Some(Instant::now());
}

/// Publish the status a finished restart left the profile in and wake the profile's SSE event consumer so every window redraws it.
///
/// ### Arguments
/// - `sync_state`: The profile's shared sync state.
/// - `status`: The status to publish.
fn publish_restart_status(sync_state: &SyncState, status: SynchronizationStatus) {
    set_sync_server_connection_status(&sync_state.connection_status, status);
    *sync_state.connecting_since.lock() = None;
    let status_tx = sync_state.sse.lock().sse_event_tx.clone();
    if let Some(tx) = status_tx {
        let _ = tx.unbounded_send(SseEvent::ConnectionStatusChanged);
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
        let sync_state = Arc::clone(&job.sync_state);
        mark_connecting(&sync_state);
        cx.notify();
        thread::spawn(move || {
            if let SseRestartOutcome::Failed(e) = job.run(None, None) {
                publish_restart_status(&sync_state, SynchronizationStatus::from_error(&e));
            }
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

        mark_connecting(&sync_state);

        // Cancelling stops the new worker too, so it does not keep retrying
        // and connect behind the user's back.
        let cancel_state = Arc::clone(&sync_state);
        let new_worker_shutdown = Arc::clone(&job.new_worker_hooks.shutdown_flag);
        let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, _cx| {
            new_worker_shutdown.store(true, Ordering::Relaxed);
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
                let (report_tx, report_rx) = mpsc::channel();
                let notification = match job.run(Some(cancel_flag), Some(report_tx)) {
                    SseRestartOutcome::Cancelled => return,
                    SseRestartOutcome::Failed(e) => {
                        publish_restart_status(&sync_state, SynchronizationStatus::from_error(&e));
                        AppNotification::background(
                            NotificationType::Error,
                            format!("{profile_name}: Connection failed: {e}"),
                        )
                    }
                    SseRestartOutcome::Started => {
                        let Some(report) = wait_for_first_attempt(&report_rx, cancel_flag) else {
                            return;
                        };
                        first_attempt_notification(&profile_name, report)
                    }
                };
                let _ = notification_tx.unbounded_send(notification);
            },
        );
    }
}
