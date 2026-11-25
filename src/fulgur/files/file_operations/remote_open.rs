use super::{
    encoding::{decode_text_file, large_file_notice},
    remote_ssh_task::{SshTaskContext, spawn_ssh_task},
    remote_types::{
        PendingRemoteOpenOutcome, RemoteBrowseResult, RemoteFileResult, RemoteOpenResult,
        RemoteOpenTaskParams, RemoteReloadGuard, SSH_CONNECTION_TIMEOUT_LABEL,
    },
};
use crate::fulgur::ui::tabs::{
    editor_tab::{LARGE_FILE_THRESHOLD_BYTES, TabLocation},
    tab::TabId,
};
use crate::fulgur::{
    Fulgur,
    shared_state::AppNotification,
    sync::ssh::{
        self,
        url::{RemoteSpec, format_remote_history_url, format_remote_url},
    },
    ui::notifications::progress::CancelCallback,
    window_manager::WindowManager,
};
use futures::channel::mpsc::UnboundedSender;
use gpui_kit::SharedString;
use gpui_kit::component::{WindowExt, notification::NotificationType};
use parking_lot::Mutex;
use std::sync::{
    Arc,
    atomic::{AtomicBool, Ordering},
};

impl Fulgur {
    /// Open a remote file from a parsed `RemoteSpec`.
    ///
    /// ### Arguments
    /// - `window`: The window to open the tab in
    /// - `cx`: The application context
    /// - `spec`: The parsed remote file specification
    pub fn do_open_remote_file(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        spec: RemoteSpec,
    ) {
        self.open_remote_file_in_new_tab(window, cx, spec, false);
    }

    /// Open a remote file from the URL submitted in the Open Remote File dialog.
    ///
    /// ### Arguments
    /// - `window`: The window to open the tab in
    /// - `cx`: The application context
    /// - `spec`: The parsed remote file specification
    pub fn open_remote_url_from_dialog(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        spec: RemoteSpec,
    ) {
        self.open_remote_file_in_new_tab(window, cx, spec, true);
    }

    /// Focus the tab already showing a remote location, or load it in a new tab.
    ///
    /// ### Arguments
    /// - `window`: The window to open the tab in
    /// - `cx`: The application context
    /// - `spec`: The parsed remote file specification
    /// - `record_in_history`: Whether the URL joins the remote history once opened
    fn open_remote_file_in_new_tab(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        spec: RemoteSpec,
        record_in_history: bool,
    ) {
        self.open_remote_file_with_target(window, cx, spec, None, record_in_history);
    }

    /// Bring forward a remote file that is already open instead of opening it twice.
    ///
    /// A tab of this window is focused. A tab of another window is reported with a
    /// notification, like an already open local file.
    ///
    /// ### Arguments
    /// - `spec`: The remote location, with its user resolved
    /// - `window`: The window the open was requested in
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: The file is already open, in this window or another one
    /// - `false`: No window has the file open
    pub(super) fn reveal_open_remote_file(
        &mut self,
        spec: &RemoteSpec,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        if let Some(tab_index) = self.find_tab_by_remote_spec(spec, cx) {
            self.set_active_tab(tab_index, window, cx);
            return true;
        }
        let Some(existing_window_id) =
            cx.global::<WindowManager>()
                .find_window_with_remote_spec(spec, self.window_id, cx)
        else {
            return false;
        };
        let file_name = spec.path.rsplit('/').next().unwrap_or(&spec.path);
        let message = format!("File '{file_name}' is already open in another window");
        window.push_notification((NotificationType::Info, SharedString::from(message)), cx);
        log::debug!(
            "Remote file {}:{} is already open in window {existing_window_id:?}",
            spec.host,
            spec.path
        );
        true
    }

    /// Start loading a remote file and apply the result either to a new tab or an existing tab.
    ///
    /// ### Arguments
    /// - `window`: The target window
    /// - `cx`: The application context
    /// - `spec`: The remote specification
    /// - `target_tab_id`: Existing tab to refresh, or `None` to open in a new tab
    /// - `record_in_history`: Whether the URL, completed with the resolved user,
    ///   joins the remote history once opened
    fn open_remote_file_with_target(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        spec: RemoteSpec,
        target_tab_id: Option<TabId>,
        record_in_history: bool,
    ) {
        let target_request_id = target_tab_id.map(|tab_id| {
            let request_id = self.next_remote_request_id;
            self.next_remote_request_id = self.next_remote_request_id.wrapping_add(1);
            self.latest_remote_open_request_by_tab
                .insert(tab_id, request_id);
            request_id
        });
        let target_reload_guard = target_tab_id.and_then(|tab_id| {
            self.tab_entity_of(tab_id, cx).and_then(|tab| {
                tab.read(cx).as_editor().and_then(|editor| {
                    let TabLocation::Remote(source) = &editor.location else {
                        return None;
                    };
                    Some(RemoteReloadGuard {
                        content_revision: editor.content_revision(cx),
                        source_url: format_remote_url(source),
                    })
                })
            })
        });

        self.with_resolved_remote_user(window, cx, spec, move |fulgur, spec, window, cx| {
            let history_url = record_in_history.then(|| format_remote_history_url(&spec));
            if target_tab_id.is_none() {
                if fulgur.reveal_open_remote_file(&spec, window, cx) {
                    if let Some(history_url) = history_url {
                        Self::record_remote_history_url(history_url, cx);
                    }
                    return;
                }
                fulgur.last_failed_remote_open_url = Some(format_remote_url(&spec));
            }
            fulgur.spawn_ssh_open_task(
                window,
                cx,
                RemoteOpenTaskParams {
                    spec,
                    target_tab_id,
                    target_request_id,
                    target_reload_guard: target_reload_guard.clone(),
                    history_url,
                },
            );
        });
    }

    /// Lazily reconnect and reload a restored remote tab when the user activates it.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    /// - `tab_id`: Stable tab id to refresh
    /// - `spec`: Remote location spec for the tab
    pub fn ensure_remote_tab_loaded(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        tab_id: TabId,
        spec: RemoteSpec,
    ) {
        self.open_remote_file_with_target(window, cx, spec, Some(tab_id), false);
    }

    /// Start reading the file of the active tab if it is still a placeholder.
    ///
    /// Does nothing for any other tab, or when that read is already running. A
    /// modified buffer is left alone, because the read would replace its text.
    /// A restored local tab normally starts its read at restore; this covers a
    /// placeholder that reached another window through a tab transfer.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    pub(crate) fn load_active_tab_pending_content(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) {
        let Some(editor_tab) = self.get_active_editor_tab(cx) else {
            return;
        };
        if !editor_tab.awaiting_content
            || editor_tab.modified
            || self.inflight_content_loads.contains(&editor_tab.id)
        {
            return;
        }
        let tab_id = editor_tab.id;
        match editor_tab.location.clone() {
            TabLocation::Remote(spec) => self.ensure_remote_tab_loaded(window, cx, tab_id, spec),
            TabLocation::Local(path) => {
                self.load_placeholder_file_content(tab_id, path, window, cx);
            }
            TabLocation::Untitled | TabLocation::Shared(_) => {}
        }
    }

    /// Refuse to write the active tab while it is a placeholder for a file.
    ///
    /// Its buffer is not the file's content, so a Save would replace the file
    /// with it and a Save As would write an empty copy. The read is started
    /// instead, so that the user can save again once the content has arrived.
    ///
    /// ### Arguments
    /// - `window`: The window context
    /// - `cx`: The application context
    ///
    /// ### Returns
    /// - `true`: The active tab is a placeholder and must not be written
    /// - `false`: The active tab can be saved
    pub(crate) fn refuse_save_of_unloaded_tab(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
    ) -> bool {
        let is_placeholder = self
            .get_active_editor_tab(cx)
            .is_some_and(|editor_tab| editor_tab.awaiting_content);
        if !is_placeholder {
            return false;
        }
        self.load_active_tab_pending_content(window, cx);
        window.push_notification(
            (
                NotificationType::Warning,
                gpui_kit::SharedString::from(
                    "This file has not loaded yet, so it was not saved. Loading it now.",
                ),
            ),
            cx,
        );
        true
    }

    /// Decode the bytes of a remote file into an open result, refusing binary content.
    ///
    /// ### Arguments
    /// - `spec`: Location the bytes were read from.
    /// - `bytes`: Raw remote file contents.
    /// - `version`: Version of the file the bytes were read from.
    ///
    /// ### Returns
    /// - `RemoteOpenResult::File`: The decoded text file.
    /// - `RemoteOpenResult::Binary`: The bytes look binary and were not decoded.
    fn remote_file_open_result(
        spec: &RemoteSpec,
        bytes: Vec<u8>,
        version: ssh::sftp::RemoteFileVersion,
    ) -> RemoteOpenResult {
        match decode_text_file(bytes) {
            Some(decoded) => RemoteOpenResult::File(RemoteFileResult {
                spec: spec.clone(),
                file_size: decoded.byte_len,
                content: decoded.content,
                encoding: decoded.encoding,
                lossy: decoded.lossy,
                version: Some(version),
            }),
            None => RemoteOpenResult::Binary(spec.clone()),
        }
    }

    /// Read a remote file into an open result, announcing it first when it is large.
    ///
    /// ### Arguments
    /// - `session`: Established SSH session with SFTP subsystem.
    /// - `spec`: Location of the file.
    /// - `remote_path`: Path to read, `spec.path` or its resolved form.
    /// - `notification_tx`: App notification channel used for the large-file notice.
    ///
    /// ### Errors
    /// - Returns an `SshError` when the file is refused or cannot be read.
    ///
    /// ### Returns
    /// - `Ok(RemoteOpenResult::File)`: The decoded text file.
    /// - `Ok(RemoteOpenResult::Binary)`: The file contents look binary.
    /// - `Err(SshError)`: The path is not a regular file, is too large, or failed to read.
    fn read_remote_file_open_result(
        session: &ssh::session::SshSession,
        spec: &RemoteSpec,
        remote_path: &str,
        notification_tx: &UnboundedSender<AppNotification>,
    ) -> Result<RemoteOpenResult, ssh::error::SshError> {
        let (bytes, version) =
            ssh::sftp::read_remote_file(session, remote_path, |reported_size| {
                if reported_size > LARGE_FILE_THRESHOLD_BYTES {
                    let file_name = remote_path.rsplit('/').next().unwrap_or(remote_path);
                    let notice = AppNotification::foreground(
                        NotificationType::Info,
                        large_file_notice(file_name),
                    );
                    if notification_tx.unbounded_send(notice).is_err() {
                        log::debug!("Notification channel closed, dropping the large-file notice");
                    }
                }
            })?;
        Ok(Self::remote_file_open_result(spec, bytes, version))
    }

    /// Resolve a remote open request into either file contents or a browse fallback.
    ///
    /// ### Arguments
    /// - `session`: Established SSH session with SFTP subsystem.
    /// - `spec`: Requested remote location.
    /// - `notification_tx`: App notification channel used for the large-file notice.
    ///
    /// ### Errors
    /// - Returns an `SshError` when classification, listing or reading fails, or when the
    ///   target is a device, a FIFO or a socket.
    ///
    /// ### Returns
    /// - `Ok(RemoteOpenResult::File)`: Target is a readable file.
    /// - `Ok(RemoteOpenResult::Binary)`: Target is a file whose contents look binary.
    /// - `Ok(RemoteOpenResult::Browse)`: Target is a directory or missing path.
    /// - `Err(SshError)`: Remote classification or I/O failure, or a non-regular file.
    fn resolve_remote_open_result(
        session: &ssh::session::SshSession,
        spec: &RemoteSpec,
        notification_tx: &UnboundedSender<AppNotification>,
    ) -> Result<RemoteOpenResult, ssh::error::SshError> {
        use ssh::sftp::{
            RemotePathKind, classify_remote_path, closest_existing_remote_directory,
            not_regular_file_error, resolve_home_relative_path,
        };

        let path = resolve_home_relative_path(session, &spec.path)?;
        match classify_remote_path(session, &path)? {
            RemotePathKind::File => {
                Self::read_remote_file_open_result(session, spec, &path, notification_tx)
            }
            RemotePathKind::Other => Err(not_regular_file_error(&path)),
            RemotePathKind::Directory => {
                Self::build_remote_browse_result(session, spec, &path, None)
            }
            RemotePathKind::Missing => {
                let fallback_directory = closest_existing_remote_directory(session, &path)?;
                let notice = Some(format!(
                    "Remote path '{}' was not found. Showing closest existing directory '{}'.",
                    spec.path, fallback_directory
                ));
                Self::build_remote_browse_result(session, spec, &fallback_directory, notice)
            }
        }
    }

    /// Build remote browser entries for a directory fallback dialog.
    ///
    /// ### Arguments
    /// - `session`: Established SSH session with SFTP subsystem.
    /// - `base_spec`: Original open request spec.
    /// - `directory_path`: Directory path to list.
    /// - `notice`: Optional informational message for the UI.
    ///
    /// ### Returns
    /// - `Ok(RemoteOpenResult::Browse)`: Browser payload with directory entries.
    /// - `Err(SshError)`: Directory listing failed.
    fn build_remote_browse_result(
        session: &ssh::session::SshSession,
        base_spec: &RemoteSpec,
        directory_path: &str,
        notice: Option<String>,
    ) -> Result<RemoteOpenResult, ssh::error::SshError> {
        let directory = if directory_path.trim().is_empty() {
            ssh::REMOTE_ROOT_PATH.to_string()
        } else {
            directory_path.to_string()
        };
        let mut directory_spec = base_spec.clone();
        directory_spec.path.clone_from(&directory);

        let mut entries: Vec<ssh::sftp::RemoteDirectoryEntry> = Vec::new();
        if directory != ssh::REMOTE_ROOT_PATH {
            let parent = ssh::sftp::parent_remote_path(&directory);
            entries.push(ssh::sftp::RemoteDirectoryEntry {
                name: "..".to_string(),
                is_dir: true,
                full_path: parent,
            });
        }
        entries.extend(ssh::sftp::list_remote_directory(session, &directory)?);

        Ok(RemoteOpenResult::Browse(RemoteBrowseResult {
            directory_spec,
            entries,
            notice,
        }))
    }

    /// Spawn the blocking SSH connect + file-read task plus a GPUI monitoring task.
    ///
    /// ### Arguments
    /// - `window`: The window context used to spawn the async monitoring task
    /// - `cx`: The application context
    /// - `params`: All data required to run the remote open operation
    fn spawn_ssh_open_task(
        &mut self,
        window: &mut gpui_kit::Window,
        cx: &mut gpui_kit::Context<Self>,
        params: RemoteOpenTaskParams,
    ) {
        let RemoteOpenTaskParams {
            spec,
            target_tab_id,
            target_request_id,
            target_reload_guard,
            history_url,
        } = params;
        if let Some(tab_id) = target_tab_id {
            self.inflight_content_loads.insert(tab_id);
        }
        let pending_remote_open = Arc::clone(&self.pending_remote_open);
        let notification_tx = Fulgur::shared_state(cx).notification_tx.clone();

        let entity_weak = cx.entity().downgrade();
        let cancel_callback: Option<CancelCallback> = Some(Box::new(move |_window, cx| {
            if let Some(entity) = entity_weak.upgrade() {
                entity.update(cx, |fulgur, _cx| {
                    if let Some(tab_id) = target_tab_id {
                        fulgur.latest_remote_open_request_by_tab.remove(&tab_id);
                        fulgur.inflight_content_loads.remove(&tab_id);
                    }
                });
            }
        }));

        spawn_ssh_task(
            window,
            cx,
            SshTaskContext {
                spec,
                progress_prefix: "Connecting to ",
                operation: "open",
                timeout_label: SSH_CONNECTION_TIMEOUT_LABEL,
                cancel_callback,
            },
            move |session, spec, _cancelled| {
                if target_tab_id.is_some() {
                    Self::read_remote_file_open_result(session, spec, &spec.path, &notification_tx)
                } else {
                    Self::resolve_remote_open_result(session, spec, &notification_tx)
                }
            },
            move |open_finished, outcome| {
                Self::publish_remote_open_outcome(
                    open_finished,
                    &pending_remote_open,
                    target_tab_id,
                    target_request_id,
                    target_reload_guard.clone(),
                    history_url.clone(),
                    outcome,
                )
            },
            |open_finished| open_finished.load(Ordering::Acquire).then_some(()),
            |_fulgur, (), _window, cx| cx.notify(),
        );
    }

    /// Publish a remote-open outcome exactly once for a single open operation.
    ///
    /// ### Arguments
    /// - `open_finished`: Per-operation completion flag shared by the worker and monitor task.
    /// - `pending_remote`: Queue of remote-open outcomes drained by the render loop.
    /// - `target_tab_id`: Existing tab id for a reload operation, or `None` for new-tab opens.
    /// - `target_request_id`: Monotonic request token used to reject stale completions.
    /// - `target_reload_guard`: Editor revision and source captured before a targeted reload.
    /// - `history_url`: Remote history URL to record if the location opens as asked.
    /// - `outcome`: Success or failure result to enqueue.
    ///
    /// ### Returns
    /// - `true`: The outcome was accepted and enqueued.
    /// - `false`: Another outcome already won the race and this one was ignored.
    pub(super) fn publish_remote_open_outcome(
        open_finished: &AtomicBool,
        pending_remote: &Mutex<Vec<PendingRemoteOpenOutcome>>,
        target_tab_id: Option<TabId>,
        target_request_id: Option<u64>,
        target_reload_guard: Option<RemoteReloadGuard>,
        history_url: Option<String>,
        outcome: Result<RemoteOpenResult, String>,
    ) -> bool {
        if open_finished.swap(true, Ordering::AcqRel) {
            false
        } else {
            pending_remote.lock().push(PendingRemoteOpenOutcome {
                target_tab_id,
                target_request_id,
                target_reload_guard,
                history_url,
                result: outcome,
            });
            true
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::{RemoteFileResult, RemoteOpenResult};
    use crate::fulgur::{Fulgur, sync::ssh::url::RemoteSpec, ui::components_utils::UTF_8};
    use parking_lot::Mutex;
    use std::sync::atomic::AtomicBool;

    fn make_remote_result() -> RemoteOpenResult {
        RemoteOpenResult::File(RemoteFileResult {
            spec: RemoteSpec {
                host: "example.com".to_string(),
                port: 22,
                user: Some("alice".to_string()),
                path: "/tmp/test.txt".to_string(),
                password_in_url: None,
            },
            content: "ok".to_string(),
            encoding: UTF_8.to_string(),
            lossy: false,
            file_size: 2,
            version: None,
        })
    }

    #[test]
    fn test_publish_remote_open_outcome_ignores_timeout_after_success() {
        let open_finished = AtomicBool::new(false);
        let pending_remote = Mutex::new(Vec::new());

        let published_success = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            None,
            None,
            Ok(make_remote_result()),
        );
        let published_timeout = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            None,
            None,
            Err("SSH connection timed out (60 s)".to_string()),
        );

        assert!(published_success, "first outcome should win");
        assert!(!published_timeout, "timeout must be ignored after success");
        let queue = pending_remote.lock();
        assert_eq!(queue.len(), 1, "only one outcome must be queued");
        assert!(
            queue[0].result.is_ok(),
            "queued outcome should be the success result"
        );
    }

    #[test]
    fn test_publish_remote_open_outcome_ignores_success_after_timeout() {
        let open_finished = AtomicBool::new(false);
        let pending_remote = Mutex::new(Vec::new());

        let published_timeout = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            None,
            None,
            Err("SSH connection timed out (60 s)".to_string()),
        );
        let published_success = Fulgur::publish_remote_open_outcome(
            &open_finished,
            &pending_remote,
            None,
            None,
            None,
            None,
            Ok(make_remote_result()),
        );

        assert!(published_timeout, "first outcome should win");
        assert!(
            !published_success,
            "late success must be ignored after timeout"
        );
        let queue = pending_remote.lock();
        assert_eq!(queue.len(), 1, "only one outcome must be queued");
        assert!(
            queue[0].result.is_err(),
            "queued outcome should be the timeout result"
        );
    }
}
