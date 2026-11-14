use crate::fulgur::sync::ssh::{
    self,
    auth::NoPrompt,
    pool::{PooledSession, SessionAttempt, retry_once_on_reused_session},
    sftp::RemoteDirectoryEntry,
    url::RemoteSpec,
};
use futures::channel::oneshot;
use std::sync::mpsc;

use super::{RemotePathBrowserConnection, paths::parse_remote_browser_input};

/// Background-worker request for one remote browser listing.
pub(super) struct BrowserListRequest {
    pub(super) raw_input: String,
    pub(super) response_tx: oneshot::Sender<Result<Vec<RemoteDirectoryEntry>, String>>,
}

/// Fetch and filter remote directory entries for one browser input value.
///
/// ### Arguments
/// - `connection`: Remote connection + credential-cache + session-pool context.
/// - `raw_input`: Path string from the browser input.
///
/// ### Returns
/// - `Ok(Vec<RemoteDirectoryEntry>)`: Entries matching the current prefix filter.
/// - `Err(String)`: If credentials are missing or remote listing failed.
fn list_entries_for_input(
    connection: &RemotePathBrowserConnection,
    raw_input: &str,
) -> Result<Vec<RemoteDirectoryEntry>, String> {
    let (directory, filter) = parse_remote_browser_input(raw_input);

    let mut entries = retry_once_on_reused_session(
        || match checkout_browser_session(connection, &directory) {
            Ok(pooled) => pooled.run(|session| {
                ssh::sftp::closest_existing_remote_directory(session, &directory)
                    .and_then(|parent| ssh::sftp::list_remote_directory(session, &parent))
                    .map_err(|e| e.user_message())
            }),
            Err(error) => SessionAttempt::checkout_failed(error),
        },
        || true,
    )?;
    if !filter.is_empty() {
        let filter_lower = filter.to_lowercase();
        entries.retain(|entry| entry.name.to_lowercase().starts_with(&filter_lower));
    }
    Ok(entries)
}

/// Check out an SSH session for one browser listing call.
///
/// ### Arguments
/// - `connection`: Remote host + credential-cache + session-pool metadata.
/// - `directory`: Current directory context used to seed the remote spec.
///
/// ### Returns
/// - `Ok(PooledSession)`: Pool-managed session ready for SFTP listings.
/// - `Err(String)`: Credentials are missing or SSH connection failed.
fn checkout_browser_session(
    connection: &RemotePathBrowserConnection,
    directory: &str,
) -> Result<PooledSession, String> {
    let cached_auth = connection
        .ssh_session_cache
        .lock()
        .get(&connection.credential_key)
        .cloned()
        .ok_or_else(|| {
            "SSH credentials are no longer available. Re-open the remote URL.".to_string()
        })?;

    let spec = RemoteSpec {
        host: connection.host.clone(),
        port: connection.port,
        user: Some(connection.user.clone()),
        path: directory.to_string(),
        password_in_url: None,
    };

    connection
        .ssh_session_pool
        .checkout_or_connect(
            &spec,
            &connection.user,
            Some(&cached_auth),
            |_, _, _| ssh::HostKeyDecision::Reject,
            &mut NoPrompt,
        )
        .map(|(session, _)| session)
        .map_err(|e| e.user_message())
}

/// Spawn a browser worker thread that pulls fresh sessions from the global pool per request.
///
/// ### Arguments
/// - `connection`: Remote connection + credential-cache + session-pool metadata.
///
/// ### Returns
/// - `mpsc::Sender<BrowserListRequest>`: Request channel for asynchronous listing jobs.
pub(super) fn spawn_browser_worker(
    connection: RemotePathBrowserConnection,
) -> mpsc::Sender<BrowserListRequest> {
    let (request_tx, request_rx) = mpsc::channel::<BrowserListRequest>();
    std::thread::spawn(move || {
        while let Ok(request) = request_rx.recv() {
            let mut latest_request = request;
            while let Ok(next) = request_rx.try_recv() {
                let _ = latest_request.response_tx.send(Err(
                    "Remote browser request superseded by newer input".to_string(),
                ));
                latest_request = next;
            }

            let result = list_entries_for_input(&connection, &latest_request.raw_input);
            let _ = latest_request.response_tx.send(result);
        }
    });
    request_tx
}
