use super::url::RemoteSpec;
use parking_lot::{Condvar, Mutex};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Coordinates remote saves so requests for one destination commit in request order.
pub struct RemoteSaveQueue {
    state: Mutex<QueueState>,
    wake: Condvar,
}

#[derive(Default)]
struct QueueState {
    destinations: HashMap<RemoteDestination, DestinationState>,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct RemoteDestination {
    host: String,
    port: u16,
    user: Option<String>,
    path: String,
}

#[derive(Default)]
struct DestinationState {
    next_ticket: u64,
    serving_ticket: u64,
    completed_tickets: HashSet<u64>,
}

/// A save request's place in its destination queue.
pub struct RemoteSavePermit {
    queue: Arc<RemoteSaveQueue>,
    destination: RemoteDestination,
    ticket: u64,
}

impl RemoteSaveQueue {
    /// Create an empty remote-save queue.
    ///
    /// ### Returns
    /// - `Self`: A queue ready to serialize remote save requests by destination.
    #[must_use]
    pub fn new() -> Self {
        Self {
            state: Mutex::new(QueueState::default()),
            wake: Condvar::new(),
        }
    }

    /// Reserve the next ordered save slot for a remote destination.
    ///
    /// ### Arguments
    /// - `spec`: Remote endpoint, user, and path that identify the destination.
    ///
    /// ### Returns
    /// - `RemoteSavePermit`: A ticket that the worker must wait on before writing.
    #[must_use]
    pub fn enqueue(self: &Arc<Self>, spec: &RemoteSpec) -> RemoteSavePermit {
        let destination = RemoteDestination {
            host: spec.host.clone(),
            port: spec.port,
            user: spec.user.clone(),
            path: spec.path.clone(),
        };
        let ticket = {
            let mut state = self.state.lock();
            let destination_state = state.destinations.entry(destination.clone()).or_default();
            let ticket = destination_state.next_ticket;
            destination_state.next_ticket = destination_state.next_ticket.wrapping_add(1);
            ticket
        };

        RemoteSavePermit {
            queue: Arc::clone(self),
            destination,
            ticket,
        }
    }

    /// Mark a ticket complete and wake any newly unblocked worker.
    ///
    /// ### Arguments
    /// - `destination`: Destination queue containing the completed ticket.
    /// - `ticket`: Ticket whose worker has finished or failed before writing.
    fn complete(&self, destination: &RemoteDestination, ticket: u64) {
        let mut state = self.state.lock();
        let remove_destination = if let Some(destination_state) =
            state.destinations.get_mut(destination)
        {
            destination_state.completed_tickets.insert(ticket);
            while destination_state
                .completed_tickets
                .remove(&destination_state.serving_ticket)
            {
                destination_state.serving_ticket = destination_state.serving_ticket.wrapping_add(1);
            }
            destination_state.serving_ticket == destination_state.next_ticket
        } else {
            false
        };
        if remove_destination {
            state.destinations.remove(destination);
        }
        self.wake.notify_all();
    }
}

impl Default for RemoteSaveQueue {
    fn default() -> Self {
        Self::new()
    }
}

impl RemoteSavePermit {
    /// Block until every earlier save request for this destination has finished.
    pub fn wait(&self) {
        let mut state = self.queue.state.lock();
        loop {
            let is_turn = state
                .destinations
                .get(&self.destination)
                .is_some_and(|destination_state| destination_state.serving_ticket == self.ticket);
            if is_turn {
                return;
            }
            self.queue.wake.wait(&mut state);
        }
    }
}

impl Drop for RemoteSavePermit {
    fn drop(&mut self) {
        self.queue.complete(&self.destination, self.ticket);
    }
}

#[cfg(test)]
mod tests {
    use super::RemoteSaveQueue;
    use crate::fulgur::sync::ssh::url::RemoteSpec;
    use parking_lot::Mutex;
    use std::{
        sync::{Arc, mpsc},
        thread,
        time::Duration,
    };

    /// Build a stable remote destination for queue tests.
    ///
    /// ### Returns
    /// - `RemoteSpec`: A password-free remote file specification.
    fn remote_spec() -> RemoteSpec {
        RemoteSpec {
            host: "example.com".to_string(),
            port: 22,
            user: Some("alice".to_string()),
            path: "/work/file.txt".to_string(),
            password_in_url: None,
        }
    }

    #[test]
    fn delayed_older_save_cannot_overwrite_newer_contents() {
        let queue = Arc::new(RemoteSaveQueue::new());
        let older_permit = queue.enqueue(&remote_spec());
        let newer_permit = queue.enqueue(&remote_spec());
        let remote_bytes = Arc::new(Mutex::new(Vec::new()));
        let (older_entered_tx, older_entered_rx) = mpsc::channel();
        let (release_older_tx, release_older_rx) = mpsc::channel();
        let (newer_waiting_tx, newer_waiting_rx) = mpsc::channel();
        let (newer_finished_tx, newer_finished_rx) = mpsc::channel();

        let older_bytes = Arc::clone(&remote_bytes);
        let older = thread::spawn(move || {
            older_permit.wait();
            older_entered_tx.send(()).unwrap();
            release_older_rx.recv().unwrap();
            *older_bytes.lock() = b"older".to_vec();
        });
        older_entered_rx.recv().unwrap();

        let newer_bytes = Arc::clone(&remote_bytes);
        let newer = thread::spawn(move || {
            newer_waiting_tx.send(()).unwrap();
            newer_permit.wait();
            *newer_bytes.lock() = b"newer".to_vec();
            newer_finished_tx.send(()).unwrap();
        });
        newer_waiting_rx.recv().unwrap();
        assert!(
            newer_finished_rx
                .recv_timeout(Duration::from_millis(50))
                .is_err(),
            "newer save must wait for the older worker to finish"
        );

        release_older_tx.send(()).unwrap();
        older.join().unwrap();
        newer.join().unwrap();
        assert_eq!(*remote_bytes.lock(), b"newer");
    }

    #[test]
    fn dropped_queued_permit_does_not_block_following_save() {
        let queue = Arc::new(RemoteSaveQueue::new());
        let first = queue.enqueue(&remote_spec());
        let failed_before_write = queue.enqueue(&remote_spec());
        let third = queue.enqueue(&remote_spec());

        drop(failed_before_write);
        first.wait();
        drop(first);
        third.wait();
    }
}
