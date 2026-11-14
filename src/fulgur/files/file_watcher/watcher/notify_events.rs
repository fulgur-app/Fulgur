use super::{FileWatchEvent, FileWatcher, PENDING_RENAME_TIMEOUT};
use futures::channel::mpsc::Sender;
use notify::{Event, EventKind};
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::Instant;

impl FileWatcher {
    /// Expire a pending rename `From` that has waited longer than `PENDING_RENAME_TIMEOUT`
    /// for its matching `To`, emitting it as a `Deleted` event.
    ///
    /// ### Arguments
    /// - `pending_rename_from`: Accumulator holding the `From` path and the instant it was stored
    /// - `event_tx`: The event sender used to emit the synthesized `Deleted` event
    pub(in crate::fulgur::files::file_watcher) fn expire_pending_rename_from(
        pending_rename_from: &Mutex<Option<(PathBuf, Instant)>>,
        event_tx: &mut Sender<FileWatchEvent>,
    ) {
        let stale = pending_rename_from.lock().ok().and_then(|mut pending| {
            let is_stale = pending
                .as_ref()
                .is_some_and(|(_, stored_at)| stored_at.elapsed() >= PENDING_RENAME_TIMEOUT);
            if is_stale { pending.take() } else { None }
        });
        if let Some((path, _)) = stale {
            Self::send_event(event_tx, FileWatchEvent::Deleted(path));
        }
    }

    /// Flush a stale pending rename `From` if it has exceeded `PENDING_RENAME_TIMEOUT`.
    pub fn flush_expired_pending_rename(&mut self) {
        Self::expire_pending_rename_from(&self.pending_rename_from, &mut self.event_tx);
    }

    /// Handles a notify event and converts it to a `FileWatchEvent`
    ///
    /// ### Description
    /// - If the event is a modification, it sends a Modified event to the event sender
    /// - If the event is a deletion, it sends a Deleted event to the event sender
    /// - If the event is a creation after a rename or save, it sends a Modified event to the event sender
    /// - If the event is a writer closing the file (Linux `IN_CLOSE_WRITE`), it sends a Modified event to the event sender
    /// - If the event is a metadata-only change (extended attributes, permissions, timestamps), it ignores it
    /// - If the event is other, it ignores it
    /// - Ignore other event types (access, etc.)
    ///
    /// Rename events come in three shapes depending on the OS backend:
    /// - **Windows**: A single event with two paths (`RenameMode::Both`)
    /// - **macOS**: A single-path `RenameMode::Any` event for the source only, since
    ///   `FSEvents` cannot pair the two sides. This also covers a move or delete-to-Trash,
    ///   so it is surfaced as a `Deleted` for the source path.
    /// - **Linux inotify**: Two consecutive single-path events (`RenameMode::From` then
    ///   `RenameMode::To`). The `pending_rename_from` accumulator pairs them up.
    ///
    /// ### Arguments
    /// - `event`: The notify event to handle
    /// - `event_tx`: The event sender to send the events to
    /// - `pending_rename_from`: Accumulator for the Linux split-rename `From` path
    pub(super) fn handle_notify_event(
        event: Event,
        event_tx: &mut Sender<FileWatchEvent>,
        pending_rename_from: &Mutex<Option<(PathBuf, Instant)>>,
    ) {
        use notify::event::{AccessKind, AccessMode, ModifyKind, RenameMode};

        // Any incoming event is a chance to flush a From whose matching To never came.
        Self::expire_pending_rename_from(pending_rename_from, event_tx);

        match event.kind {
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                if event.paths.len() == 2 {
                    // macOS / Windows: both paths arrive in one event
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    Self::send_event(event_tx, FileWatchEvent::Renamed { from, to });
                } else if event.paths.len() == 1 {
                    match rename_mode {
                        RenameMode::From => {
                            // Linux inotify: first half - store and wait for the To event
                            if let Ok(mut pending) = pending_rename_from.lock() {
                                *pending = Some((event.paths[0].clone(), Instant::now()));
                            }
                        }
                        RenameMode::To => {
                            // Linux inotify: second half - pair with the stored From path
                            let from = pending_rename_from
                                .lock()
                                .ok()
                                .and_then(|mut p| p.take())
                                .map(|(path, _)| path);
                            match from {
                                Some(from) => {
                                    Self::send_event(
                                        event_tx,
                                        FileWatchEvent::Renamed {
                                            from,
                                            to: event.paths[0].clone(),
                                        },
                                    );
                                }
                                None => {
                                    // No matching From; treat as a new file appearing
                                    Self::send_event(
                                        event_tx,
                                        FileWatchEvent::Modified(event.paths[0].clone()),
                                    );
                                }
                            }
                        }
                        _ => {
                            // Orphaned or unrecognised single-path rename; flush any pending From
                            if let Ok(mut pending) = pending_rename_from.lock()
                                && let Some((stale, _)) = pending.take()
                            {
                                Self::send_event(event_tx, FileWatchEvent::Deleted(stale));
                            }
                            // macOS FSEvents reports a move, rename, or delete-to-Trash of a
                            // watched file as a single-path `RenameMode::Any` with no destination.
                            if rename_mode == RenameMode::Any {
                                Self::send_event(
                                    event_tx,
                                    FileWatchEvent::Deleted(event.paths[0].clone()),
                                );
                            }
                        }
                    }
                }
            }
            // macOS sets extended attributes such as `com.apple.provenance` on files
            // some time after they are written, which must not count as an edit.
            EventKind::Modify(ModifyKind::Metadata(_)) => {}
            // Linux inotify reports a writer closing the file, which marks the end of
            // a write whose last data events may have been coalesced away.
            EventKind::Modify(_)
            | EventKind::Create(_)
            | EventKind::Access(AccessKind::Close(AccessMode::Write)) => {
                for path in event.paths {
                    Self::send_event(event_tx, FileWatchEvent::Modified(path));
                }
            }
            EventKind::Remove(_) => {
                for path in event.paths {
                    Self::send_event(event_tx, FileWatchEvent::Deleted(path));
                }
            }
            _ => {}
        }
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::super::{FILE_WATCH_EVENT_CHANNEL_CAPACITY, FileWatchEvent, FileWatcher};
    use crate::fulgur::files::file_watcher::test_helpers::temp_test_path;
    use futures::channel::mpsc::{TryRecvError, channel};
    use gpui_kit::TestAppContext;
    use notify::{
        Event, EventKind,
        event::{
            AccessKind, AccessMode, DataChange, MetadataKind, ModifyKind, RemoveKind, RenameMode,
        },
    };
    use std::{
        sync::Mutex,
        time::{Duration, Instant},
    };

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_modify_to_modified(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_modify.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Modified(actual)) if actual == path
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_close_write_to_modified(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_close_write.txt");
        let event = Event::new(EventKind::Access(AccessKind::Close(AccessMode::Write)))
            .add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Modified(actual)) if actual == path),
            "a writer closing the file marks the end of a write and must be reported"
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_remove_to_deleted(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_deleted.txt");
        let event = Event::new(EventKind::Remove(RemoveKind::File)).add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Deleted(actual)) if actual == path
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_macos_rename_any_to_deleted(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_rename_any.txt");
        // macOS FSEvents reports a delete-to-Trash, move, or rename of a watched
        // file as a single-path `RenameMode::Any` with no destination.
        let event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Any))).add_path(path.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == path),
            "a single-path RenameMode::Any should map to a Deleted event for the source path"
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_maps_rename_both_to_renamed(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_old.txt");
        let to = temp_test_path("fulgur_notify_new.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::Both)))
            .add_path(from.clone())
            .add_path(to.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Renamed {
                from: actual_from,
                to: actual_to
            }) if actual_from == from && actual_to == to
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_pairs_linux_split_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_linux_from.txt");
        let to = temp_test_path("fulgur_notify_linux_to.txt");
        let from_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(from.clone());
        FileWatcher::handle_notify_event(from_event, &mut event_tx, &pending_rename_from);
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
        let to_event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(to.clone());
        FileWatcher::handle_notify_event(to_event, &mut event_tx, &pending_rename_from);
        assert!(matches!(
            event_rx.try_recv(),
            Ok(FileWatchEvent::Renamed {
                from: actual_from,
                to: actual_to
            }) if actual_from == from && actual_to == to
        ));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_expires_stale_pending_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let stale_from = temp_test_path("fulgur_notify_stale_from.txt");
        let stored_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("instant subtraction should not underflow");
        let pending_rename_from = Mutex::new(Some((stale_from.clone(), stored_at)));
        let unrelated = temp_test_path("fulgur_notify_unrelated_modify.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Data(DataChange::Content)))
            .add_path(unrelated.clone());
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == stale_from),
            "a stale pending From should be flushed as Deleted on the next event"
        );
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Modified(actual)) if actual == unrelated)
        );
        assert!(
            pending_rename_from.lock().expect("lock poisoned").is_none(),
            "the stale pending From should be cleared after expiry"
        );
    }

    #[gpui_kit::test]
    fn test_flush_expired_pending_rename_emits_deleted(_cx: &mut TestAppContext) {
        let (mut watcher, mut event_rx) = FileWatcher::new();
        let stale_from = temp_test_path("fulgur_flush_stale_from.txt");
        let stored_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("instant subtraction should not underflow");
        *watcher.pending_rename_from.lock().expect("lock poisoned") =
            Some((stale_from.clone(), stored_at));
        watcher.flush_expired_pending_rename();
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == stale_from),
            "flush should expire a never-completed rename From"
        );
        assert!(
            watcher
                .pending_rename_from
                .lock()
                .expect("lock poisoned")
                .is_none()
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_ignores_metadata_only_changes(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_metadata.txt");
        let event = Event::new(EventKind::Modify(ModifyKind::Metadata(
            MetadataKind::Extended,
        )))
        .add_path(path);
        FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Err(TryRecvError::Empty)),
            "an extended-attribute change must not be reported as a modification"
        );
    }
}
