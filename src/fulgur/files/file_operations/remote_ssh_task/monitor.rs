use crate::fulgur::{
    Fulgur,
    sync::ssh::{auth::CredentialRequest, session::HostKeyRequest},
    ui::notifications::progress::ProgressNotification,
};
use parking_lot::Mutex;
use std::{
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::JoinHandle,
    time::Duration,
};

/// Interval at which the monitor task checks for completion and user prompts.
const MONITOR_POLL_INTERVAL: Duration = Duration::from_millis(100);

/// Outcome published when the worker thread ends without publishing one, i.e. it panicked.
const SSH_WORKER_STOPPED_MESSAGE: &str = "SSH worker stopped unexpectedly";

/// State the UI monitor task shares with an SSH worker thread.
pub(super) struct SshWorkerHandles {
    /// The worker thread; once it has finished, a missing outcome means it panicked.
    pub(super) worker: JoinHandle<()>,
    pub(super) pending_host_key: Arc<Mutex<Option<HostKeyRequest>>>,
    pub(super) pending_credential: Arc<Mutex<Option<CredentialRequest>>>,
    /// Completion flag guarding the exactly-once `publish`.
    pub(super) finished: Arc<AtomicBool>,
    /// Set when the user cancels from the progress notification.
    pub(super) cancel_flag: Arc<AtomicBool>,
    /// Operation and endpoint named in the logs.
    pub(super) log_label: String,
}

/// Watch an SSH worker thread until it publishes an outcome, is cancelled, or stops.
///
/// ### Arguments
/// - `window`: The window used to spawn the monitor task and show the prompt dialogs
/// - `cx`: The application context
/// - `progress`: Progress notification, dismissed when the monitor ends
/// - `handles`: State shared with the worker thread
/// - `publish`: Records an outcome exactly once, guarded by the shared completion flag
/// - `poll_completion`: Returns the payload to deliver once the operation has completed
/// - `on_complete`: Applies the delivered payload on the UI thread
pub(super) fn spawn_ssh_monitor<Work, Payload, Publish>(
    window: &mut gpui_kit::Window,
    cx: &mut gpui_kit::Context<Fulgur>,
    progress: ProgressNotification,
    handles: SshWorkerHandles,
    publish: Arc<Publish>,
    poll_completion: impl Fn(&AtomicBool) -> Option<Payload> + 'static,
    on_complete: impl Fn(&mut Fulgur, Payload, &mut gpui_kit::Window, &mut gpui_kit::Context<Fulgur>)
    + 'static,
) where
    Payload: 'static,
    Publish: Fn(&AtomicBool, Result<Work, String>) -> bool + 'static,
{
    let SshWorkerHandles {
        worker,
        pending_host_key,
        pending_credential,
        finished,
        cancel_flag,
        log_label,
    } = handles;

    cx.spawn_in(window, async move |view, async_cx| {
        let _progress = progress;
        loop {
            async_cx
                .background_executor()
                .timer(MONITOR_POLL_INTERVAL)
                .await;

            let completion = if let Some(payload) = poll_completion(&finished) {
                Some(payload)
            } else if cancel_flag.load(Ordering::Acquire) {
                None
            } else if worker.is_finished() {
                // A worker that returns always publishes, so it panicked. The exactly-once
                // guard keeps an outcome it published after the poll above.
                if (*publish)(&finished, Err(SSH_WORKER_STOPPED_MESSAGE.to_string())) {
                    log::error!("{log_label} failed: {SSH_WORKER_STOPPED_MESSAGE}");
                }
                poll_completion(&finished)
            } else {
                let host_key_request = pending_host_key.lock().take();
                let credential_request = pending_credential.lock().take();
                if host_key_request.is_some() || credential_request.is_some() {
                    async_cx
                        .update(|window, cx| {
                            _ = view.update(cx, |fulgur, cx| {
                                if let Some(request) = host_key_request {
                                    fulgur.show_ssh_host_fingerprint_dialog(window, cx, request);
                                }
                                if let Some(request) = credential_request {
                                    fulgur.show_ssh_credential_dialog(window, cx, request);
                                }
                            });
                        })
                        .ok();
                }
                continue;
            };

            if let Some(payload) = completion {
                async_cx
                    .update(|window, cx| {
                        _ = view.update(cx, |fulgur, cx| {
                            on_complete(fulgur, payload, window, cx);
                        });
                    })
                    .ok();
            }
            break;
        }
    })
    .detach();
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod gpui_monitor_tests {
    use super::{
        MONITOR_POLL_INTERVAL, SSH_WORKER_STOPPED_MESSAGE, SshWorkerHandles, spawn_ssh_monitor,
    };
    use crate::fulgur::{
        Fulgur,
        files::file_operations::{
            remote_types::SSH_USER_PROMPT_TIMEOUT, test_helpers::setup_fulgur_with_root,
        },
        ui::notifications::progress::start_progress,
    };
    use gpui_kit::{Entity, VisualTestContext};
    use parking_lot::Mutex;
    use std::{
        cell::RefCell,
        rc::Rc,
        sync::{Arc, atomic::AtomicBool, mpsc},
        thread::{self, JoinHandle},
        time::{Duration, Instant},
    };

    /// Save outcome slot shared by the worker's `publish` and the monitor's poll.
    type PendingSave = Arc<Mutex<Option<Result<(), String>>>>;

    /// Longest time a test waits on its worker thread.
    const WORKER_DEADLINE: Duration = Duration::from_secs(5);

    /// Watch `worker` with a save monitor and record the outcome it delivers.
    ///
    /// ### Arguments
    /// - `visual_cx`: The visual test context driving the window
    /// - `fulgur`: The window's Fulgur entity
    /// - `worker`: The worker thread to watch
    /// - `finished`: Completion flag shared with the worker
    /// - `pending_save`: Outcome slot shared with the worker
    ///
    /// ### Returns
    /// - `Rc<RefCell<Option<Result<(), String>>>>`: Filled with the outcome once the
    ///   monitor delivers it on the UI thread
    fn start_save_monitor(
        visual_cx: &mut VisualTestContext,
        fulgur: &Entity<Fulgur>,
        worker: JoinHandle<()>,
        finished: Arc<AtomicBool>,
        pending_save: PendingSave,
    ) -> Rc<RefCell<Option<Result<(), String>>>> {
        let delivered = Rc::new(RefCell::new(None));
        let delivered_by_monitor = Rc::clone(&delivered);
        let pending_save_for_publish = Arc::clone(&pending_save);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |_, cx| {
                let progress = start_progress(window, cx, "Saving".into(), None);
                let cancel_flag = progress.cancel_flag();
                spawn_ssh_monitor(
                    window,
                    cx,
                    progress,
                    SshWorkerHandles {
                        worker,
                        pending_host_key: Arc::default(),
                        pending_credential: Arc::default(),
                        finished,
                        cancel_flag,
                        log_label: "Remote save of /tmp/notes.txt".to_string(),
                    },
                    Arc::new(move |finished: &AtomicBool, outcome: Result<(), String>| {
                        Fulgur::publish_remote_save_outcome(
                            finished,
                            &pending_save_for_publish,
                            outcome,
                        )
                    }),
                    move |_| pending_save.lock().take(),
                    move |_, result, _, _| {
                        *delivered_by_monitor.borrow_mut() = Some(result);
                    },
                );
            });
        });
        delivered
    }

    #[gpui_kit::test]
    fn test_ssh_monitor_waits_for_worker_past_prompt_timeout(cx: &mut gpui_kit::TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        let finished = Arc::new(AtomicBool::new(false));
        let pending_save = PendingSave::default();
        let (release_tx, release_rx) = mpsc::channel::<()>();
        let (published_tx, published_rx) = mpsc::channel::<()>();
        let worker = {
            let finished = Arc::clone(&finished);
            let pending_save = Arc::clone(&pending_save);
            thread::spawn(move || {
                release_rx.recv().ok();
                Fulgur::publish_remote_save_outcome(&finished, &pending_save, Ok(()));
                published_tx.send(()).ok();
            })
        };
        let delivered = start_save_monitor(
            &mut visual_cx,
            &fulgur,
            worker,
            Arc::clone(&finished),
            pending_save,
        );

        visual_cx
            .background_executor
            .advance_clock(SSH_USER_PROMPT_TIMEOUT * 2);
        visual_cx.run_until_parked();
        assert!(
            delivered.borrow().is_none(),
            "a worker still transferring must not be reported as timed out"
        );

        release_tx
            .send(())
            .expect("worker is waiting to be released");
        published_rx
            .recv_timeout(WORKER_DEADLINE)
            .expect("worker published its outcome");
        visual_cx
            .background_executor
            .advance_clock(MONITOR_POLL_INTERVAL);
        visual_cx.run_until_parked();
        assert_eq!(*delivered.borrow(), Some(Ok(())));
    }

    #[gpui_kit::test]
    fn test_ssh_monitor_reports_worker_that_stopped_without_outcome(
        cx: &mut gpui_kit::TestAppContext,
    ) {
        let (fulgur, mut visual_cx) = setup_fulgur_with_root(cx);
        // Ends without publishing, as a panicking worker does.
        let worker = thread::spawn(|| {});
        let deadline = Instant::now() + WORKER_DEADLINE;
        while !worker.is_finished() {
            assert!(Instant::now() < deadline, "the worker thread never ended");
            thread::sleep(Duration::from_millis(1));
        }
        let delivered = start_save_monitor(
            &mut visual_cx,
            &fulgur,
            worker,
            Arc::new(AtomicBool::new(false)),
            PendingSave::default(),
        );

        visual_cx
            .background_executor
            .advance_clock(MONITOR_POLL_INTERVAL);
        visual_cx.run_until_parked();
        assert_eq!(
            *delivered.borrow(),
            Some(Err(SSH_WORKER_STOPPED_MESSAGE.to_string()))
        );
    }
}
