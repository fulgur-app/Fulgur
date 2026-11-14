use crate::fulgur::sync::{
    sse::types::SseEvent,
    synchronization::{SynchronizationStatus, set_sync_server_connection_status},
};
use futures::channel::mpsc::UnboundedSender;
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    time::Duration,
};

/// Publish a connection status and wake the windows displaying it.
///
/// ### Arguments
/// - `shutdown_flag`: The worker's shutdown flag.
/// - `connection_status`: The profile's shared connection status.
/// - `event_tx`: Channel carrying the change to the UI-thread consumer.
/// - `status`: The status to publish.
pub(super) fn publish_connection_status(
    shutdown_flag: &AtomicBool,
    connection_status: &Arc<Mutex<SynchronizationStatus>>,
    event_tx: &UnboundedSender<SseEvent>,
    status: SynchronizationStatus,
) {
    if shutdown_flag.load(Ordering::Relaxed) {
        return;
    }
    if *connection_status.lock() == status {
        return;
    }
    set_sync_server_connection_status(connection_status, status);
    event_tx
        .unbounded_send(SseEvent::ConnectionStatusChanged)
        .ok();
}

/// Consecutive failed attempts still reported as "connecting" before a profile
/// is shown as disconnected.
const TRANSIENT_RETRY_ATTEMPTS: u32 = 5;

/// Resolve the status to publish after a failed attempt that will be retried.
///
/// ### Arguments
/// - `consecutive_failures`: Failed attempts since the last successful connect.
///
/// ### Returns
/// - `None`: The outage still looks transient; leave the status as it is.
/// - `Some(SynchronizationStatus::Disconnected)`: Retries have gone on long
///   enough that the profile should be reported as down.
pub(super) fn retry_status(consecutive_failures: u32) -> Option<SynchronizationStatus> {
    (consecutive_failures > TRANSIENT_RETRY_ATTEMPTS).then_some(SynchronizationStatus::Disconnected)
}

/// Status Fulgurant answers with while this device's previous SSE stream still
/// holds a slot against the per-device connection cap.
const SSE_SLOT_BUSY_STATUS: u16 = 429;

/// Delay between attempts while the previous stream is still releasing its slot.
pub(super) const SSE_SLOT_BUSY_RETRY_DELAY: Duration = Duration::from_millis(500);

/// Attempts spent at the fast handoff cadence before the standard backoff takes over.
const SSE_SLOT_BUSY_FAST_ATTEMPTS: u32 = 10;

/// Decide whether a failed connect is a slot handoff worth retrying quickly.
///
/// ### Arguments
/// - `error`: The error the connect attempt failed with.
/// - `attempts_so_far`: Fast handoff attempts already spent since the last success.
///
/// ### Returns
/// - `true`: The previous stream is still releasing its slot; retry quickly.
/// - `false`: Treat the attempt as a real failure and back off.
pub(super) fn is_handoff_retry(error: &ureq::Error, attempts_so_far: u32) -> bool {
    matches!(error, ureq::Error::StatusCode(SSE_SLOT_BUSY_STATUS))
        && attempts_so_far < SSE_SLOT_BUSY_FAST_ATTEMPTS
}

#[cfg(test)]
mod tests {
    use super::{
        Arc, AtomicBool, Mutex, Ordering, SSE_SLOT_BUSY_FAST_ATTEMPTS, SSE_SLOT_BUSY_STATUS,
        SseEvent, SynchronizationStatus, TRANSIENT_RETRY_ATTEMPTS, is_handoff_retry,
        publish_connection_status, retry_status,
    };
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};

    /// Count the `ConnectionStatusChanged` events queued on a receiver.
    fn queued_status_changes(events: &mut UnboundedReceiver<SseEvent>) -> usize {
        let mut count = 0;
        while let Ok(event) = events.try_recv() {
            if matches!(event, SseEvent::ConnectionStatusChanged) {
                count += 1;
            }
        }
        count
    }

    #[test]
    fn a_running_worker_publishes_its_status_and_wakes_the_ui() {
        let shutdown = AtomicBool::new(false);
        let status = Arc::new(Mutex::new(SynchronizationStatus::Connecting));
        let (tx, mut rx) = unbounded();
        publish_connection_status(&shutdown, &status, &tx, SynchronizationStatus::Connected);
        assert_eq!(*status.lock(), SynchronizationStatus::Connected);
        assert_eq!(
            queued_status_changes(&mut rx),
            1,
            "a status change must be announced so the windows repaint"
        );
    }

    #[test]
    fn republishing_the_same_status_does_not_wake_the_ui() {
        let shutdown = AtomicBool::new(false);
        let status = Arc::new(Mutex::new(SynchronizationStatus::Disconnected));
        let (tx, mut rx) = unbounded();
        publish_connection_status(&shutdown, &status, &tx, SynchronizationStatus::Disconnected);
        assert_eq!(
            queued_status_changes(&mut rx),
            0,
            "a reconnect loop must not wake the UI on every retry"
        );
    }

    #[test]
    fn a_retiring_worker_cannot_overwrite_its_replacement() {
        // The replacement worker has published Connected on the shared cell;
        // the worker being retired must not downgrade it on its way out.
        let shutdown = AtomicBool::new(false);
        let status = Arc::new(Mutex::new(SynchronizationStatus::Connected));
        let (tx, mut rx) = unbounded();
        shutdown.store(true, Ordering::Relaxed);
        publish_connection_status(&shutdown, &status, &tx, SynchronizationStatus::Disconnected);
        assert_eq!(
            *status.lock(),
            SynchronizationStatus::Connected,
            "a worker asked to stop must not write to the shared status cell"
        );
        assert_eq!(queued_status_changes(&mut rx), 0);
    }

    #[test]
    fn a_reconnect_in_progress_leaves_the_status_alone() {
        // The profile is usable once the initial synchronization succeeds, so a
        // stream reconnecting in the background must not pull it out of
        // Connected and contradict the notification the user just saw.
        for attempt in 1..=TRANSIENT_RETRY_ATTEMPTS {
            assert_eq!(
                retry_status(attempt),
                None,
                "attempt {attempt} is still within the transient window"
            );
        }
    }

    #[test]
    fn a_sustained_outage_reads_as_disconnected() {
        assert_eq!(
            retry_status(TRANSIENT_RETRY_ATTEMPTS + 1),
            Some(SynchronizationStatus::Disconnected),
            "retries that stop looking transient must report the profile as down"
        );
    }

    #[test]
    fn a_busy_slot_is_retried_on_the_fast_cadence() {
        // The server answers 429 while the stream being replaced still counts
        // against the per-device cap. Backing off exponentially there would
        // overshoot a release that happens within a moment.
        let busy = ureq::Error::StatusCode(SSE_SLOT_BUSY_STATUS);
        assert!(is_handoff_retry(&busy, 0));
        assert!(is_handoff_retry(&busy, SSE_SLOT_BUSY_FAST_ATTEMPTS - 1));
    }

    #[test]
    fn a_slot_that_never_frees_falls_back_to_backoff() {
        let busy = ureq::Error::StatusCode(SSE_SLOT_BUSY_STATUS);
        assert!(
            !is_handoff_retry(&busy, SSE_SLOT_BUSY_FAST_ATTEMPTS),
            "the fast cadence must be bounded so a stuck slot stops being polled"
        );
    }

    #[test]
    fn a_real_failure_is_never_treated_as_a_handoff() {
        assert!(!is_handoff_retry(&ureq::Error::StatusCode(500), 0));
        assert!(!is_handoff_retry(&ureq::Error::HostNotFound, 0));
    }
}
