use super::{FileWatchEvent, FileWatcher, PENDING_RENAME_TIMEOUT, PendingRenameFrom};
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
    /// - `pending_rename_from`: Accumulator holding the pending `From` and the instant it was stored
    /// - `event_tx`: The event sender used to emit the synthesized `Deleted` event
    pub(in crate::fulgur::files::file_watcher) fn expire_pending_rename_from(
        pending_rename_from: &Mutex<Option<PendingRenameFrom>>,
        event_tx: &mut Sender<FileWatchEvent>,
    ) {
        let stale = pending_rename_from.lock().ok().and_then(|mut pending| {
            let is_stale = pending
                .as_ref()
                .is_some_and(|p| p.stored_at.elapsed() >= PENDING_RENAME_TIMEOUT);
            if is_stale { pending.take() } else { None }
        });
        if let Some(stale) = stale {
            Self::send_event(event_tx, FileWatchEvent::Deleted(stale.path));
        }
    }

    /// Flush a stale pending rename `From` if it has exceeded `PENDING_RENAME_TIMEOUT`.
    pub fn flush_expired_pending_rename(&mut self) {
        Self::expire_pending_rename_from(&self.pending_rename_from, &mut self.event_tx);
    }

    /// Resolve the second half of a split rename against the stored `From`
    ///
    /// ### Arguments
    /// - `from`: The pending `From`, if any
    /// - `to`: The path reported by the `To` event
    /// - `to_tracker`: The tracker carried by the `To` event
    ///
    /// ### Returns
    /// - `Vec<FileWatchEvent>`: The events to emit, in order
    fn resolve_rename_to(
        from: Option<PendingRenameFrom>,
        to: PathBuf,
        to_tracker: Option<usize>,
    ) -> Vec<FileWatchEvent> {
        match from {
            None => vec![FileWatchEvent::Modified(to)],
            Some(from) if from.path == to => vec![FileWatchEvent::Modified(to)],
            Some(from) if from.tracker.is_some() && from.tracker == to_tracker => {
                vec![FileWatchEvent::Renamed {
                    from: from.path,
                    to,
                }]
            }
            Some(from) => vec![
                FileWatchEvent::Deleted(from.path),
                FileWatchEvent::Modified(to),
            ],
        }
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
    /// Rename events come in several shapes depending on the OS backend:
    /// - **Two-path event** (`RenameMode::Both`): inotify, once it has paired both sides itself
    /// - **macOS**: A single-path `RenameMode::Any` event for the source only, since
    ///   `FSEvents` cannot pair the two sides. This also covers a move or delete-to-Trash,
    ///   so it is surfaced as a `Deleted` for the source path.
    /// - **Linux inotify and Windows**: Two consecutive single-path events (`RenameMode::From`
    ///   then `RenameMode::To`). The `pending_rename_from` accumulator holds the `From` and
    ///   `resolve_rename_to` decides what the pair means. inotify tags both halves with a
    ///   cookie; Windows does not, so its halves are never paired into a rename.
    ///
    /// ### Arguments
    /// - `event`: The notify event to handle
    /// - `event_tx`: The event sender to send the events to
    /// - `pending_rename_from`: Accumulator for the split-rename `From` half
    pub(super) fn handle_notify_event(
        event: Event,
        event_tx: &mut Sender<FileWatchEvent>,
        pending_rename_from: &Mutex<Option<PendingRenameFrom>>,
    ) {
        use notify::event::{AccessKind, AccessMode, ModifyKind, RenameMode};

        // Any incoming event is a chance to flush a From whose matching To never came.
        Self::expire_pending_rename_from(pending_rename_from, event_tx);

        match event.kind {
            EventKind::Modify(ModifyKind::Name(rename_mode)) => {
                if event.paths.len() == 2 {
                    // Both paths arrive in one event, already paired by the backend
                    let from = event.paths[0].clone();
                    let to = event.paths[1].clone();
                    Self::send_event(event_tx, FileWatchEvent::Renamed { from, to });
                } else if event.paths.len() == 1 {
                    match rename_mode {
                        RenameMode::From => {
                            // First half: store and wait for the To event. A From that was
                            // still waiting can no longer be paired, so flush it as deleted.
                            let replaced = pending_rename_from.lock().ok().and_then(|mut p| {
                                p.replace(PendingRenameFrom {
                                    path: event.paths[0].clone(),
                                    tracker: event.attrs.tracker(),
                                    stored_at: Instant::now(),
                                })
                            });
                            if let Some(replaced) = replaced {
                                Self::send_event(event_tx, FileWatchEvent::Deleted(replaced.path));
                            }
                        }
                        RenameMode::To => {
                            let from = pending_rename_from.lock().ok().and_then(|mut p| p.take());
                            for resolved in Self::resolve_rename_to(
                                from,
                                event.paths[0].clone(),
                                event.attrs.tracker(),
                            ) {
                                Self::send_event(event_tx, resolved);
                            }
                        }
                        _ => {
                            // Orphaned or unrecognised single-path rename; flush any pending From
                            if let Ok(mut pending) = pending_rename_from.lock()
                                && let Some(stale) = pending.take()
                            {
                                Self::send_event(event_tx, FileWatchEvent::Deleted(stale.path));
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
    use super::super::{
        FILE_WATCH_EVENT_CHANNEL_CAPACITY, FileWatchEvent, FileWatcher, PendingRenameFrom,
    };
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
            .add_path(from.clone())
            .set_tracker(42);
        FileWatcher::handle_notify_event(from_event, &mut event_tx, &pending_rename_from);
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
        let to_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To)))
            .add_path(to.clone())
            .set_tracker(42);
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
    fn test_handle_notify_event_same_path_split_rename_is_modified(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let path = temp_test_path("fulgur_notify_safe_save.txt");
        // Windows safe save by another program: the original is renamed aside, then a
        // temp file is renamed over it. A single-file watch only sees the watched path.
        let from_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(path.clone());
        FileWatcher::handle_notify_event(from_event, &mut event_tx, &pending_rename_from);
        let to_event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(path.clone());
        FileWatcher::handle_notify_event(to_event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Modified(actual)) if actual == path),
            "a file replaced through renames must be re-checked, not treated as a rename"
        );
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_untracked_split_rename_is_not_paired(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_untracked_from.txt");
        let to = temp_test_path("fulgur_notify_untracked_to.txt");
        // Windows sets no tracker, so two unrelated watched files must not be merged.
        let from_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(from.clone());
        FileWatcher::handle_notify_event(from_event, &mut event_tx, &pending_rename_from);
        let to_event =
            Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To))).add_path(to.clone());
        FileWatcher::handle_notify_event(to_event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == from)
        );
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Modified(actual)) if actual == to)
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_mismatched_trackers_are_not_paired(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let from = temp_test_path("fulgur_notify_mismatch_from.txt");
        let to = temp_test_path("fulgur_notify_mismatch_to.txt");
        let from_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
            .add_path(from.clone())
            .set_tracker(1);
        FileWatcher::handle_notify_event(from_event, &mut event_tx, &pending_rename_from);
        let to_event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::To)))
            .add_path(to.clone())
            .set_tracker(2);
        FileWatcher::handle_notify_event(to_event, &mut event_tx, &pending_rename_from);
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == from)
        );
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Modified(actual)) if actual == to)
        );
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_second_from_flushes_first(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let pending_rename_from = Mutex::new(None);
        let first = temp_test_path("fulgur_notify_first_from.txt");
        let second = temp_test_path("fulgur_notify_second_from.txt");
        for path in [&first, &second] {
            let event = Event::new(EventKind::Modify(ModifyKind::Name(RenameMode::From)))
                .add_path(path.clone());
            FileWatcher::handle_notify_event(event, &mut event_tx, &pending_rename_from);
        }
        assert!(
            matches!(event_rx.try_recv(), Ok(FileWatchEvent::Deleted(actual)) if actual == first),
            "a From replaced before its To arrived must not be dropped silently"
        );
        assert!(matches!(event_rx.try_recv(), Err(TryRecvError::Empty)));
    }

    #[gpui_kit::test]
    fn test_handle_notify_event_expires_stale_pending_rename(_cx: &mut TestAppContext) {
        let (mut event_tx, mut event_rx) = channel(FILE_WATCH_EVENT_CHANNEL_CAPACITY);
        let stale_from = temp_test_path("fulgur_notify_stale_from.txt");
        let stored_at = Instant::now()
            .checked_sub(Duration::from_secs(1))
            .expect("instant subtraction should not underflow");
        let pending_rename_from = Mutex::new(Some(PendingRenameFrom {
            path: stale_from.clone(),
            tracker: None,
            stored_at,
        }));
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
        *watcher.pending_rename_from.lock().expect("lock poisoned") = Some(PendingRenameFrom {
            path: stale_from.clone(),
            tracker: None,
            stored_at,
        });
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
