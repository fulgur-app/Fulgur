use crate::fulgur::state::persistence::{DeviceKeyPins, RemoteHistory, WindowsState};
use parking_lot::Mutex;

/// Coalescing hand-off for asynchronous snapshots, remote histories and device key pins.
///
/// Only the newest snapshot matters, since each one describes the whole session,
/// so a burst collapses into a single write instead of queueing one snapshot per
/// request. The remote history follows the same rule in its own slot, so a
/// window snapshot never overwrites a history waiting to be written. Device
/// key pins get a third slot under the same rule.
#[derive(Default)]
pub(super) struct SnapshotMailbox {
    pending: Mutex<Option<WindowsState>>,
    remote_history: Mutex<Option<RemoteHistory>>,
    device_key_pins: Mutex<Option<DeviceKeyPins>>,
}

impl SnapshotMailbox {
    /// Store a snapshot, replacing any snapshot already waiting.
    ///
    /// ### Arguments
    /// - `state`: The snapshot to hand to the writer thread.
    pub(super) fn put(&self, state: WindowsState) {
        *self.pending.lock() = Some(state);
    }

    /// Take the waiting snapshot, leaving the mailbox empty.
    ///
    /// ### Returns
    /// - `Some(WindowsState)`: The snapshot that was waiting
    /// - `None`: The mailbox was empty
    pub(super) fn take(&self) -> Option<WindowsState> {
        self.pending.lock().take()
    }

    /// Report whether a snapshot is waiting.
    ///
    /// ### Returns
    /// - `bool`: `true` when the mailbox holds a snapshot to write
    pub(super) fn has_pending(&self) -> bool {
        self.pending.lock().is_some()
    }

    /// Store a remote history, replacing any history already waiting.
    ///
    /// ### Arguments
    /// - `history`: The history to hand to the writer thread.
    pub(super) fn put_remote_history(&self, history: RemoteHistory) {
        *self.remote_history.lock() = Some(history);
    }

    /// Take the waiting remote history, leaving its slot empty.
    ///
    /// ### Returns
    /// - `Some(RemoteHistory)`: The history that was waiting
    /// - `None`: No history was waiting
    pub(super) fn take_remote_history(&self) -> Option<RemoteHistory> {
        self.remote_history.lock().take()
    }

    /// Store device key pins, replacing any pins already waiting.
    ///
    /// ### Arguments
    /// - `pins`: The pins to hand to the writer thread.
    pub(super) fn put_device_key_pins(&self, pins: DeviceKeyPins) {
        *self.device_key_pins.lock() = Some(pins);
    }

    /// Take the waiting device key pins, leaving their slot empty.
    ///
    /// ### Returns
    /// - `Some(DeviceKeyPins)`: The pins that were waiting
    /// - `None`: No pins were waiting
    pub(super) fn take_device_key_pins(&self) -> Option<DeviceKeyPins> {
        self.device_key_pins.lock().take()
    }
}
