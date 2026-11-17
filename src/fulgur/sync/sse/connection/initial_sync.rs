use crate::fulgur::{
    settings::ServerProfile,
    shared_state::{AppNotification, SyncState},
    sync::{
        sse::types::SseEvent,
        synchronization::{
            SynchronizationError, SynchronizationStatus, apply_initial_sync_outcome,
            initial_synchronization,
        },
    },
    utils::retry::{BackoffCalculator, interruptible_sleep},
};
use futures::channel::mpsc::UnboundedSender;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
    mpsc,
};

use super::status::publish_connection_status;

/// Outcome of the first initial synchronization attempt, for a caller that
/// reports it to the user (the connection progress indicator).
pub enum InitialSyncReport {
    /// The synchronization succeeded; the SSE stream is being opened.
    Synced {
        /// The name the server registered for this device.
        device_name: String,
        /// Version notifications the synchronization warrants.
        notifications: Vec<AppNotification>,
    },
    /// The synchronization failed.
    Failed {
        /// Why the attempt failed.
        error: SynchronizationError,
        /// Whether the worker keeps retrying in the background.
        retrying: bool,
    },
}

/// Initial synchronization an SSE worker runs before opening its stream.
///
/// Running it on the worker thread, instead of before spawning the worker,
/// keeps a failed synchronization under the worker's lifecycle: transient
/// failures are retried with backoff until they succeed or the worker is
/// told to stop, so an offline launch or a server blip is not terminal.
pub struct InitialSyncStage {
    /// The profile's shared sync state, updated with the synchronization outcome.
    pub sync_state: Arc<SyncState>,
    /// Receives the first attempt's outcome. Later outcomes, and the first one
    /// when this is `None`, are only published as a status and notifications.
    pub first_attempt_tx: Option<mpsc::Sender<InitialSyncReport>>,
}

/// How a retried initial synchronization ended.
#[derive(Debug)]
enum RetriedSync<T> {
    /// An attempt succeeded.
    Synced(T),
    /// An attempt failed permanently; retrying would repeat the failure.
    GaveUp,
    /// The worker was told to stop before an attempt succeeded.
    Stopped,
}

/// Run `attempt` until it succeeds, fails permanently, or a stop is requested.
///
/// ### Arguments
/// - `shutdown_flag`: The worker's shutdown flag, checked before each attempt
///   and polled during the backoff between attempts.
/// - `backoff`: Delay schedule between failed attempts.
/// - `attempt`: One synchronization attempt.
/// - `on_failure`: Called with each failure and whether it will be retried.
///
/// ### Returns
/// - `RetriedSync::Synced(T)`: The value of the successful attempt.
/// - `RetriedSync::GaveUp`: An attempt failed with a permanent error.
/// - `RetriedSync::Stopped`: The shutdown flag was set before an attempt succeeded.
fn retry_until_synchronized<T>(
    shutdown_flag: &AtomicBool,
    backoff: &mut BackoffCalculator,
    mut attempt: impl FnMut() -> Result<T, SynchronizationError>,
    mut on_failure: impl FnMut(SynchronizationError, bool),
) -> RetriedSync<T> {
    let should_stop = || shutdown_flag.load(Ordering::Relaxed);
    loop {
        if should_stop() {
            return RetriedSync::Stopped;
        }
        let error = match attempt() {
            Ok(value) => return RetriedSync::Synced(value),
            Err(error) => error,
        };
        let retrying = error.is_transient();
        on_failure(error, retrying);
        if !retrying {
            return RetriedSync::GaveUp;
        }
        if interruptible_sleep(backoff.record_failure(), should_stop) {
            return RetriedSync::Stopped;
        }
    }
}

impl InitialSyncStage {
    /// Synchronize with the profile's server, retrying transient failures, and
    /// publish every outcome.
    ///
    /// ### Arguments
    /// - `profile`: The profile to synchronize.
    /// - `http_agent`: Agent for the synchronization's REST calls.
    /// - `shutdown_flag`: The worker's shutdown flag; a set flag ends the
    ///   retries and keeps a retired worker from publishing a status.
    /// - `event_tx`: Channel waking the windows that display the status.
    ///
    /// ### Returns
    /// - `true`: The synchronization succeeded; the worker should open its stream.
    /// - `false`: It failed permanently or the worker was told to stop.
    pub(super) fn run(
        self,
        profile: &ServerProfile,
        http_agent: &ureq::Agent,
        shutdown_flag: &AtomicBool,
        event_tx: &UnboundedSender<SseEvent>,
    ) -> bool {
        let Self {
            sync_state,
            mut first_attempt_tx,
        } = self;
        let publish = |status: SynchronizationStatus| {
            if shutdown_flag.load(Ordering::Relaxed) {
                return;
            }
            *sync_state.connecting_since.lock() = None;
            publish_connection_status(
                shutdown_flag,
                &sync_state.connection_status,
                event_tx,
                status,
            );
        };
        let mut backoff = BackoffCalculator::default_settings();
        let outcome = retry_until_synchronized(
            shutdown_flag,
            &mut backoff,
            || {
                initial_synchronization(
                    profile,
                    &sync_state.token_state,
                    http_agent,
                    &sync_state.pending_ack_share_ids,
                )
            },
            |error, retrying| {
                if retrying {
                    log::warn!(
                        "Profile '{}': initial synchronization failed, retrying: {error}",
                        profile.name
                    );
                } else {
                    log::error!(
                        "Profile '{}': initial synchronization failed: {error}",
                        profile.name
                    );
                }
                publish(SynchronizationStatus::from_error(&error));
                if let Some(tx) = first_attempt_tx.take() {
                    let _ = tx.send(InitialSyncReport::Failed { error, retrying });
                }
            },
        );
        let RetriedSync::Synced(outcome) = outcome else {
            return false;
        };
        log::info!("Profile '{}': connected to sync server", profile.name);
        let (device_name, notifications) =
            apply_initial_sync_outcome(&sync_state, &profile.name, outcome);
        publish(SynchronizationStatus::Connected);
        match first_attempt_tx {
            Some(tx) => {
                let _ = tx.send(InitialSyncReport::Synced {
                    device_name,
                    notifications,
                });
            }
            None => notifications
                .into_iter()
                .for_each(|notification| sync_state.notify(notification)),
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::{AtomicBool, BackoffCalculator, Ordering, RetriedSync, retry_until_synchronized};
    use crate::fulgur::sync::synchronization::SynchronizationError;
    use std::time::Duration;

    fn instant_backoff() -> BackoffCalculator {
        BackoffCalculator::new(Duration::ZERO, Duration::ZERO, 1.0)
    }

    #[test]
    fn transient_failures_are_retried_until_the_sync_succeeds() {
        let shutdown = AtomicBool::new(false);
        let mut attempts = 0;
        let mut failures = Vec::new();
        let outcome = retry_until_synchronized(
            &shutdown,
            &mut instant_backoff(),
            || {
                attempts += 1;
                match attempts {
                    1 => Err(SynchronizationError::HostNotFound),
                    2 => Err(SynchronizationError::ServerError(503)),
                    _ => Ok(attempts),
                }
            },
            |error, retrying| failures.push((error.to_string(), retrying)),
        );
        assert!(matches!(outcome, RetriedSync::Synced(3)));
        assert_eq!(failures.len(), 2);
        assert!(
            failures.iter().all(|(_, retrying)| *retrying),
            "transient failures must be reported as retried"
        );
    }

    #[test]
    fn a_permanent_failure_ends_the_retries() {
        let shutdown = AtomicBool::new(false);
        let mut attempts = 0;
        let mut failures = Vec::new();
        let outcome = retry_until_synchronized(
            &shutdown,
            &mut instant_backoff(),
            || -> Result<(), SynchronizationError> {
                attempts += 1;
                Err(SynchronizationError::AuthenticationFailed)
            },
            |error, retrying| failures.push((error.to_string(), retrying)),
        );
        assert!(matches!(outcome, RetriedSync::GaveUp));
        assert_eq!(attempts, 1, "a rejected login must not be retried");
        assert_eq!(failures.len(), 1);
        assert!(!failures[0].1, "a permanent failure must not claim a retry");
    }

    #[test]
    fn a_stopped_worker_makes_no_attempt() {
        let shutdown = AtomicBool::new(true);
        let mut attempts = 0;
        let outcome = retry_until_synchronized(
            &shutdown,
            &mut instant_backoff(),
            || -> Result<(), SynchronizationError> {
                attempts += 1;
                Ok(())
            },
            |_, _| {},
        );
        assert!(matches!(outcome, RetriedSync::Stopped));
        assert_eq!(attempts, 0);
    }

    #[test]
    fn a_stop_during_the_backoff_ends_the_retries() {
        let shutdown = AtomicBool::new(false);
        let mut attempts = 0;
        let mut long_backoff =
            BackoffCalculator::new(Duration::from_mins(1), Duration::from_mins(1), 1.0);
        let outcome = retry_until_synchronized(
            &shutdown,
            &mut long_backoff,
            || -> Result<(), SynchronizationError> {
                attempts += 1;
                Err(SynchronizationError::ConnectionFailed)
            },
            |_, _| shutdown.store(true, Ordering::Relaxed),
        );
        assert!(matches!(outcome, RetriedSync::Stopped));
        assert_eq!(attempts, 1, "the backoff must be interrupted by the stop");
    }

    #[test]
    fn only_outages_and_server_side_failures_are_transient() {
        let transient = [
            SynchronizationError::ConnectionFailed,
            SynchronizationError::HostNotFound,
            SynchronizationError::Timeout("read".to_string()),
            SynchronizationError::ServerError(500),
            SynchronizationError::ServerError(429),
        ];
        let permanent = [
            SynchronizationError::AuthenticationFailed,
            SynchronizationError::DeviceKeyMissing,
            SynchronizationError::ServerTooOld { required: "0.9.0" },
            SynchronizationError::ServerUrlMissing,
            SynchronizationError::ServerError(404),
        ];
        for error in transient {
            assert!(error.is_transient(), "{error:?} should be retried");
        }
        for error in permanent {
            assert!(!error.is_transient(), "{error:?} should not be retried");
        }
    }
}
