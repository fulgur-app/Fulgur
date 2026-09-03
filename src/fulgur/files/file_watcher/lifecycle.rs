use super::watcher::{FileWatchEvent, FileWatcher, PENDING_RENAME_TIMEOUT};
use crate::fulgur::Fulgur;
use crate::fulgur::tab::Tab;
use futures::StreamExt;
use futures::channel::mpsc::{Receiver, Sender};
use futures::future::Either;
use gpui::{Context, Task};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

impl Fulgur {
    /// Start the file watcher, watch all open files, and spawn the event consumer task
    ///
    /// ### Arguments
    /// - `cx`: The application context used to spawn the consumer task
    pub fn start_file_watcher(&mut self, cx: &mut Context<Self>) {
        let (mut watcher, receiver) = FileWatcher::new();
        if let Err(e) = watcher.start() {
            log::error!("Failed to start file watcher: {e}");
            return;
        }
        for tab in &self.tabs {
            if let Tab::Editor(editor_tab) = tab.read(cx)
                && let Some(path) = editor_tab.file_path()
                && let Err(e) = watcher.watch_file(path)
            {
                log::warn!("Failed to watch file {}: {}", path.display(), e);
            }
        }
        let pending_rename_from = Arc::clone(&watcher.pending_rename_from);
        let flush_tx = watcher.event_tx.clone();
        self.file_watch_state.file_watcher = Some(watcher);
        self.file_watch_state.consumer_task =
            Some(self.spawn_file_watch_consumer(receiver, pending_rename_from, flush_tx, cx));
    }

    /// Stop the file watcher and cancel the event consumer task
    pub fn stop_file_watcher(&mut self) {
        if let Some(mut watcher) = self.file_watch_state.file_watcher.take() {
            watcher.stop();
        }
        self.file_watch_state.consumer_task = None;
        self.clear_file_watch_bookkeeping();
    }

    /// Add a file to the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to watch
    pub fn watch_file(&mut self, path: &std::path::Path) {
        if let Some(watcher) = &mut self.file_watch_state.file_watcher
            && let Err(e) = watcher.watch_file(&path.to_path_buf())
        {
            log::warn!("Failed to watch file {}: {}", path.display(), e);
        }
    }

    /// Remove a file from the watcher
    ///
    /// ### Arguments
    /// - `path`: The path to the file to unwatch
    pub fn unwatch_file(&mut self, path: &PathBuf) {
        if let Some(watcher) = &mut self.file_watch_state.file_watcher {
            watcher.unwatch_file(path);
        }
        self.prune_file_watch_bookkeeping_for_path(path);
    }

    /// Spawn the task that consumes file watch events for this window
    ///
    /// ### Arguments
    /// - `events`: The receiver side of the watcher's event channel
    /// - `pending_rename_from`: The watcher's pending split-rename accumulator
    /// - `flush_tx`: Sender used to emit the synthesized `Deleted` event on expiry
    /// - `cx`: The application context used to spawn the task
    ///
    /// ### Returns
    /// - `Task<()>`: The consumer task; dropping it cancels the consumer
    fn spawn_file_watch_consumer(
        &self,
        mut events: Receiver<FileWatchEvent>,
        pending_rename_from: Arc<Mutex<Option<(PathBuf, Instant)>>>,
        mut flush_tx: Sender<FileWatchEvent>,
        cx: &mut Context<Self>,
    ) -> Task<()> {
        let window_id = self.window_id;
        cx.spawn(async move |view, cx| {
            loop {
                let has_pending_rename = pending_rename_from
                    .lock()
                    .is_ok_and(|pending| pending.is_some());
                let event = if has_pending_rename {
                    let timer = cx.background_executor().timer(PENDING_RENAME_TIMEOUT);
                    match futures::future::select(events.next(), std::pin::pin!(timer)).await {
                        Either::Left((event, _)) => event,
                        Either::Right(((), _)) => {
                            FileWatcher::expire_pending_rename_from(
                                &pending_rename_from,
                                &mut flush_tx,
                            );
                            continue;
                        }
                    }
                } else {
                    events.next().await
                };
                let Some(event) = event else {
                    break;
                };
                let handle = cx.update(|cx| {
                    cx.windows()
                        .into_iter()
                        .find(|handle| handle.window_id() == window_id)
                });
                let Some(handle) = handle else {
                    break;
                };
                let delivered = handle.update(cx, |_, window, cx| {
                    view.update(cx, |this, cx| {
                        this.handle_file_watch_event(event, window, cx);
                        cx.notify();
                    })
                });
                if !matches!(delivered, Ok(Ok(()))) {
                    break;
                }
            }
        })
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::files::file_watcher::test_helpers::{setup_fulgur, temp_test_path};
    use gpui::TestAppContext;
    use std::time::Instant;

    #[gpui::test]
    fn test_unwatch_file_prunes_bookkeeping_maps(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_unwatch_cleanup.txt");
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .pending_conflicts
                    .insert(path.clone(), 0);
                this.unwatch_file(&path);
                assert!(
                    !this.file_watch_state.last_file_events.contains_key(&path),
                    "unwatch must prune debounce bookkeeping"
                );
                assert!(
                    !this.file_watch_state.last_file_saves.contains_key(&path),
                    "unwatch must prune save bookkeeping"
                );
                assert!(
                    !this.file_watch_state.pending_conflicts.contains_key(&path),
                    "unwatch must prune deferred conflict bookkeeping"
                );
            });
        });
    }

    #[gpui::test]
    fn test_stop_file_watcher_clears_bookkeeping_maps(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        let path = temp_test_path("fulgur_stop_watcher_cleanup.txt");
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, _cx| {
                this.file_watch_state
                    .last_file_events
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .last_file_saves
                    .insert(path.clone(), Instant::now());
                this.file_watch_state
                    .pending_conflicts
                    .insert(path.clone(), 0);
                this.stop_file_watcher();
                assert!(
                    this.file_watch_state.last_file_events.is_empty(),
                    "stopping watcher must clear debounce bookkeeping"
                );
                assert!(
                    this.file_watch_state.last_file_saves.is_empty(),
                    "stopping watcher must clear save bookkeeping"
                );
                assert!(
                    this.file_watch_state.pending_conflicts.is_empty(),
                    "stopping watcher must clear deferred conflict bookkeeping"
                );
            });
        });
    }
}
