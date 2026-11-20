//! Single-instance coordination for Windows and Linux.
//!
//! When a new Fulgur process is launched with a file-path argument (taskbar
//! jump list on Windows, double-click / "Open with" on Linux), this module
//! detects the already-running instance, forwards the path to it, and lets the
//! caller exit early. Windows uses a named pipe scoped to the user and logon
//! session, with a user-only DACL and peer-user checks on both ends (see
//! `windows_pipe`); Linux uses a private Unix domain socket. Both channels are
//! keyed by a hash of the configuration directory (see `instance_key`), so an
//! installed Fulgur, a portable Fulgur and a dev instance started with `-d`
//! stay isolated from each other.
//!
//! Each accepted connection is read on its own short-lived thread, so a client
//! that connects and never closes cannot block the listener.
//!
//! The listening instance receives the path, appends it to the shared
//! `pending_files` queue and signals the `wake` channel so the app opens /
//! focuses the file in the last focused window - the same queue and channel
//! used by the macOS "Open With" handler.
//!
//! Files forwarded by a `--read-only` launch are prefixed with `RO:` so that
//! only those files open read-only in the running instance.
//!
//! Jump list Tasks ("New Tab", "New Window") send a `CMD:new-tab` /
//! `CMD:new-window` line instead of a file path. The listener pushes these into
//! `pending_ipc_commands`, and the same wake delivers them in-process.

#[cfg(target_os = "windows")]
use crate::fulgur::utils::windows_pipe::{self, PipeListener as IpcListener};
use crate::fulgur::utils::{instance_key, paths};
use crate::fulgur::{
    shared_state::ExternalOpenFile,
    utils::{
        ipc_protocol::{command_line, file_line, read_ipc_connection},
        worker::Worker,
    },
};
use futures::channel::mpsc::UnboundedSender;
use parking_lot::Mutex;
#[cfg(target_os = "windows")]
use std::fs::File as IpcStream;
#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixListener as IpcListener, UnixStream as IpcStream};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{
    io::{self, BufReader, Read, Write},
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    thread,
    time::Duration,
};

/// Read timeout of an accepted Unix socket connection, bounding the lifetime of
/// its reader thread when a client never closes.
#[cfg(target_os = "linux")]
const IPC_READ_TIMEOUT: Duration = Duration::from_secs(5);

/// Pause after a failed accept, so a persistent error cannot spin the listener.
const IPC_ACCEPT_ERROR_BACKOFF: Duration = Duration::from_millis(100);

/// Maximum time a dropped IPC listener worker is joined before being detached.
/// The wakeup self-connect unblocks `accept` immediately, so this is short.
const IPC_WORKER_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolve the name of this installation's IPC pipe.
///
/// ### Errors
/// Returns an `io::Error` if the configuration directory, the user SID or the
/// session id cannot be resolved.
///
/// ### Returns
/// - `Ok(String)`: The pipe name, scoped to the user, session and configuration directory
/// - `Err(io::Error)`: The pipe name could not be built
#[cfg(target_os = "windows")]
fn pipe_name() -> io::Result<String> {
    let config_dir = paths::config_dir().map_err(io::Error::other)?;
    windows_pipe::pipe_name(&instance_key::instance_id(&config_dir))
}

/// Resolve the path of the Unix domain socket used for IPC.
///
/// ### Returns
/// - `Ok(PathBuf)`: The socket path derived from the configuration directory
/// - `Err(io::Error)`: The configuration directory could not be resolved
#[cfg(target_os = "linux")]
fn socket_path() -> io::Result<PathBuf> {
    let config_dir = paths::config_dir().map_err(io::Error::other)?;
    let runtime_dir = std::env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .filter(|dir| dir.is_absolute());
    let fallback_dir = if crate::fulgur::utils::portable::is_portable() {
        std::env::temp_dir()
    } else {
        config_dir.clone()
    };
    Ok(instance_key::socket_path(
        &config_dir,
        runtime_dir.as_deref(),
        &fallback_dir,
    ))
}

/// Connect to the IPC listener of the running instance.
///
/// On Windows the pipe server must run as the current user, otherwise the pipe
/// is treated as absent so this process starts normally.
///
/// ### Returns
/// - `Ok(IpcStream)`: A connection to the running instance
/// - `Err(io::Error)`: No trusted instance is listening
fn connect() -> io::Result<IpcStream> {
    #[cfg(target_os = "windows")]
    return windows_pipe::connect_trusted(&pipe_name()?);
    #[cfg(target_os = "linux")]
    return IpcStream::connect(socket_path()?);
}

/// Connect to this instance's own listener so a pending `accept` returns on shutdown.
fn unblock_own_listener() {
    #[cfg(target_os = "windows")]
    if let Ok(name) = pipe_name() {
        let _ = windows_pipe::connect(&name);
    }
    #[cfg(target_os = "linux")]
    if let Ok(path) = socket_path() {
        let _ = IpcStream::connect(path);
    }
}

/// Bind the IPC listener of this instance.
///
/// ### Returns
/// - `Ok(IpcListener)`: The bound listener
/// - `Err(io::Error)`: The listener could not be bound
fn bind() -> io::Result<IpcListener> {
    #[cfg(target_os = "windows")]
    return IpcListener::bind(&pipe_name()?);
    #[cfg(target_os = "linux")]
    return bind_socket(&socket_path()?);
}

/// Wait for the next connection on the IPC listener.
///
/// ### Arguments
/// - `listener`: The bound listener
///
/// ### Returns
/// - `Ok(IpcStream)`: The accepted connection
/// - `Err(io::Error)`: The accept failed, or (Windows) the client was not trusted
fn accept(listener: &mut IpcListener) -> io::Result<IpcStream> {
    #[cfg(target_os = "windows")]
    return listener.accept();
    #[cfg(target_os = "linux")]
    return listener.accept().map(|(stream, _)| {
        if let Err(e) = stream.set_read_timeout(Some(IPC_READ_TIMEOUT)) {
            log::warn!("Could not set the IPC connection read timeout: {e}");
        }
        stream
    });
}

/// Bind a Unix domain socket listener, recovering from a crashed instance.
///
/// ### Arguments
/// - `path`: The socket path to bind
///
/// ### Returns
/// - `Ok(IpcListener)`: The bound listener
/// - `Err(io::Error)`: The listener could not be bound, or another instance is listening
#[cfg(target_os = "linux")]
fn bind_socket(path: &Path) -> io::Result<IpcListener> {
    match IpcListener::bind(path) {
        Err(e) if e.kind() == io::ErrorKind::AddrInUse && IpcStream::connect(path).is_err() => {
            log::info!("Removing stale single-instance socket: {}", path.display());
            std::fs::remove_file(path)?;
            IpcListener::bind(path)
        }
        result => result,
    }
}

/// Write one file line per path to a connection to the running instance.
///
/// ### Arguments
/// - `stream`: The connection to the running instance
/// - `paths`: The file paths to forward
/// - `read_only`: Whether the running instance must open these files read-only
///
/// ### Errors
/// Returns an `io::Error` if a line cannot be written or flushed.
///
/// ### Returns
/// - `Ok(())`: Every line was written
/// - `Err(io::Error)`: The connection failed before every line was written
fn forward_files(mut stream: impl Write, paths: &[PathBuf], read_only: bool) -> io::Result<()> {
    for path in paths {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        writeln!(stream, "{}", file_line(&path, read_only))?;
    }
    stream.flush()
}

/// Try to forward `paths` to an already-running Fulgur instance.
///
/// Connects to the listener started by the primary instance. If the
/// connection succeeds the paths are written one per line and the caller
/// should exit immediately. If the connection is refused this is the first
/// instance and the caller should continue normally.
///
/// ### Arguments
/// - `paths`: The file paths to forward to the running instance
/// - `read_only`: Whether the running instance must open these files read-only
///
/// ### Returns
/// - `true`: Another instance was found and the paths were forwarded - caller should exit
/// - `false`: No existing instance is running, or forwarding failed - caller should continue
#[must_use]
pub fn try_forward_to_existing_instance(paths: &[PathBuf], read_only: bool) -> bool {
    let Ok(stream) = connect() else {
        return false;
    };
    match forward_files(stream, paths, read_only) {
        Ok(()) => {
            log::info!(
                "Single-instance: forwarded {} path(s) to running instance",
                paths.len()
            );
            true
        }
        Err(e) => {
            log::warn!("Single-instance: forwarding paths failed, starting normally: {e}");
            false
        }
    }
}

/// Try to send a command to an already-running Fulgur instance.
///
/// Writes a single `CMD:<cmd>` line (e.g. `CMD:new-tab`) to the running
/// instance's listener. If the write succeeds the command has been delivered
/// and the caller should exit immediately. If there is no existing instance,
/// or the write fails, the caller should start normally.
///
/// ### Arguments
/// - `cmd`: The command identifier to send (e.g. `"new-tab"`, `"new-window"`)
///
/// ### Returns
/// - `true`: Another instance was found and the command was forwarded - caller should exit
/// - `false`: No existing instance is running, or sending failed - caller should continue
#[must_use]
pub fn try_send_command_to_existing_instance(cmd: &str) -> bool {
    let Ok(mut stream) = connect() else {
        return false;
    };
    match writeln!(stream, "{}", command_line(cmd)).and_then(|()| stream.flush()) {
        Ok(()) => {
            log::info!("Single-instance: forwarded command '{cmd}' to running instance");
            true
        }
        Err(e) => {
            log::warn!("Single-instance: sending command '{cmd}' failed, starting normally: {e}");
            false
        }
    }
}

/// Start listening for messages from new Fulgur processes.
///
/// File-path lines are appended to `pending_files`, mirroring the macOS
/// "Open With" path. Lines prefixed with `CMD:` are appended to
/// `pending_ipc_commands` for in-process actions such as opening a new tab or
/// window. After each connection is read, a message is sent on `wake` so the
/// app delivers both queues to the last focused window.
///
/// ### Arguments
/// - `pending_files`: Shared queue to receive file paths forwarded by other instances
/// - `pending_ipc_commands`: Shared queue to receive command strings forwarded by other instances
/// - `wake`: Channel signalled once a forwarded message has been queued
///
/// ### Returns
/// - `Some(Worker)`: The Drop-owned listener worker; dropping it stops the
///   listener (the wakeup self-connects to unblock the pending `accept`).
/// - `None`: The listener could not be bound.
#[must_use]
pub fn start_ipc_listener(
    pending_files: Arc<Mutex<Vec<ExternalOpenFile>>>,
    pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    wake: UnboundedSender<()>,
) -> Option<Worker> {
    let listener = match bind() {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Could not start single-instance IPC listener: {e}");
            return None;
        }
    };
    Some(spawn_ipc_listener(
        listener,
        unblock_own_listener,
        pending_files,
        pending_ipc_commands,
        wake,
    ))
}

/// Spawn the worker thread accepting connections on a bound listener.
///
/// ### Arguments
/// - `listener`: The bound listener, moved into the worker
/// - `unblock_accept`: Connects to `listener` so a pending `accept` returns on shutdown
/// - `pending_files`: Shared queue to receive file paths forwarded by other instances
/// - `pending_ipc_commands`: Shared queue to receive command strings forwarded by other instances
/// - `wake`: Channel signalled after each connection has been read
///
/// ### Returns
/// - `Worker`: The Drop-owned listener worker
fn spawn_ipc_listener(
    mut listener: IpcListener,
    unblock_accept: impl Fn() + Send + Sync + 'static,
    pending_files: Arc<Mutex<Vec<ExternalOpenFile>>>,
    pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    wake: UnboundedSender<()>,
) -> Worker {
    Worker::spawn(
        "fulgur-ipc-listener",
        IPC_WORKER_JOIN_TIMEOUT,
        move |shutdown| {
            loop {
                let stream = accept(&mut listener);
                if shutdown.load(Ordering::Relaxed) {
                    log::info!("IPC listener shutdown requested, stopping");
                    break;
                }
                match stream {
                    Ok(stream) => spawn_connection_reader(
                        stream,
                        Arc::clone(&pending_files),
                        Arc::clone(&pending_ipc_commands),
                        wake.clone(),
                    ),
                    Err(e) => {
                        log::warn!("IPC listener accept error: {e}");
                        thread::sleep(IPC_ACCEPT_ERROR_BACKOFF);
                    }
                }
            }
        },
    )
    .with_wakeup(unblock_accept)
}

/// Read one accepted connection on its own thread, then signal `wake`.
///
/// Reading off the listener thread means a client that connects and never
/// closes only ties up its own reader, never the accept loop.
///
/// ### Arguments
/// - `stream`: The accepted connection
/// - `pending_files`: Shared queue to receive the forwarded file paths
/// - `pending_ipc_commands`: Shared queue to receive the forwarded commands
/// - `wake`: Channel signalled once the connection has been read
fn spawn_connection_reader(
    stream: impl Read + Send + 'static,
    pending_files: Arc<Mutex<Vec<ExternalOpenFile>>>,
    pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    wake: UnboundedSender<()>,
) {
    let spawned = thread::Builder::new()
        .name("fulgur-ipc-connection".to_string())
        .spawn(move || {
            read_ipc_connection(
                BufReader::new(stream),
                &pending_files,
                &pending_ipc_commands,
            );
            let _ = wake.unbounded_send(());
        });
    if let Err(e) = spawned {
        log::warn!("Could not spawn an IPC connection reader: {e}");
    }
}

#[cfg(all(test, target_os = "linux"))]
mod tests {
    use super::{IPC_WORKER_JOIN_TIMEOUT, bind_socket, forward_files, spawn_ipc_listener};
    use crate::fulgur::{
        shared_state::ExternalOpenFile,
        utils::{ipc_protocol::command_line, worker::Worker},
    };
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use parking_lot::Mutex;
    use std::{
        io::{ErrorKind, Write},
        os::unix::net::{UnixListener, UnixStream},
        path::Path,
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };

    /// Queues and wake receiver of a listener started on a test socket.
    struct TestListener {
        worker: Worker,
        pending_files: Arc<Mutex<Vec<ExternalOpenFile>>>,
        pending_ipc_commands: Arc<Mutex<Vec<String>>>,
        wake_rx: UnboundedReceiver<()>,
    }

    /// Bind `socket` and start a listener on it, as `start_ipc_listener` does on the real socket.
    ///
    /// ### Arguments
    /// - `socket`: The socket path to listen on
    ///
    /// ### Returns
    /// - `TestListener`: The running worker with its queues and wake receiver
    fn start_test_listener(socket: &Path) -> TestListener {
        let listener = bind_socket(socket).expect("failed to bind test socket");
        let pending_files = Arc::new(Mutex::new(Vec::new()));
        let pending_ipc_commands = Arc::new(Mutex::new(Vec::new()));
        let (wake_tx, wake_rx) = unbounded();
        let unblock_socket = socket.to_path_buf();
        let worker = spawn_ipc_listener(
            listener,
            move || {
                let _ = UnixStream::connect(&unblock_socket);
            },
            Arc::clone(&pending_files),
            Arc::clone(&pending_ipc_commands),
            wake_tx,
        );
        TestListener {
            worker,
            pending_files,
            pending_ipc_commands,
            wake_rx,
        }
    }

    /// Wait until the listener signals that it has read a connection.
    ///
    /// ### Arguments
    /// - `wake_rx`: The listener's wake receiver
    ///
    /// ### Returns
    /// - `true`: A wake arrived within five seconds
    /// - `false`: No wake arrived
    fn wait_for_wake(wake_rx: &mut UnboundedReceiver<()>) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if wake_rx.try_recv().is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn test_listener_queues_forwarded_files_and_commands() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("fulgur.sock");
        let existing = dir.path().join("existing.txt");
        std::fs::write(&existing, "content").unwrap();
        let missing = dir.path().join("missing.txt");
        let mut listener = start_test_listener(&socket);

        forward_files(
            UnixStream::connect(&socket).unwrap(),
            &[existing.clone(), missing],
            false,
        )
        .unwrap();
        assert!(wait_for_wake(&mut listener.wake_rx), "files were not read");
        let mut stream = UnixStream::connect(&socket).unwrap();
        writeln!(stream, "{}", command_line("new-tab")).unwrap();
        drop(stream);
        assert!(wait_for_wake(&mut listener.wake_rx), "command was not read");

        assert_eq!(
            *listener.pending_files.lock(),
            vec![ExternalOpenFile {
                path: std::fs::canonicalize(&existing).unwrap(),
                read_only: false,
            }]
        );
        assert_eq!(
            *listener.pending_ipc_commands.lock(),
            vec!["new-tab".to_string()]
        );
    }

    #[test]
    fn test_bind_socket_replaces_a_stale_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("fulgur.sock");
        drop(UnixListener::bind(&socket).unwrap());
        assert!(socket.exists(), "a dropped listener leaves its socket file");

        let listener = bind_socket(&socket).expect("a stale socket must be replaced");

        assert!(UnixStream::connect(&socket).is_ok());
        drop(listener);
    }

    #[test]
    fn test_bind_socket_keeps_a_live_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("fulgur.sock");
        let _live = UnixListener::bind(&socket).unwrap();

        let error = bind_socket(&socket).expect_err("a live socket must not be taken over");

        assert_eq!(error.kind(), ErrorKind::AddrInUse);
        assert!(UnixStream::connect(&socket).is_ok());
    }

    #[test]
    fn test_dropping_the_listener_worker_joins_and_closes_the_socket() {
        let dir = tempfile::tempdir().unwrap();
        let socket = dir.path().join("fulgur.sock");
        let listener = start_test_listener(&socket);

        let started = Instant::now();
        drop(listener.worker);

        assert!(
            started.elapsed() < IPC_WORKER_JOIN_TIMEOUT,
            "the wakeup must unblock accept so the worker joins instead of timing out"
        );
        assert!(
            UnixStream::connect(&socket).is_err(),
            "the listener socket must be closed once the worker has exited"
        );
    }
}

#[cfg(all(test, target_os = "windows"))]
mod windows_tests {
    use super::{IPC_WORKER_JOIN_TIMEOUT, forward_files, spawn_ipc_listener};
    use crate::fulgur::{
        shared_state::ExternalOpenFile,
        utils::{
            ipc_protocol::command_line,
            windows_pipe::{self, PipeListener},
        },
    };
    use futures::channel::mpsc::{UnboundedReceiver, unbounded};
    use parking_lot::Mutex;
    use std::{
        io::{ErrorKind, Write},
        sync::Arc,
        thread,
        time::{Duration, Instant},
    };

    /// Build a pipe name unique to one test, so tests never collide with a running Fulgur.
    ///
    /// ### Returns
    /// - `String`: A fresh pipe name
    fn test_pipe_name() -> String {
        format!(r"\\.\pipe\Fulgur-test-{}", uuid::Uuid::new_v4())
    }

    /// Wait until the listener signals that it has read a connection.
    ///
    /// ### Arguments
    /// - `wake_rx`: The listener's wake receiver
    ///
    /// ### Returns
    /// - `true`: A wake arrived within five seconds
    /// - `false`: No wake arrived
    fn wait_for_wake(wake_rx: &mut UnboundedReceiver<()>) -> bool {
        let deadline = Instant::now() + Duration::from_secs(5);
        while Instant::now() < deadline {
            if wake_rx.try_recv().is_ok() {
                return true;
            }
            thread::sleep(Duration::from_millis(10));
        }
        false
    }

    #[test]
    fn test_pipe_listener_queues_files_and_commands_despite_a_stuck_client() {
        let name = test_pipe_name();
        let dir = tempfile::tempdir().unwrap();
        let existing = dir.path().join("existing.txt");
        std::fs::write(&existing, "content").unwrap();
        let listener = PipeListener::bind(&name).expect("failed to create test pipe");
        let pending_files = Arc::new(Mutex::new(Vec::new()));
        let pending_ipc_commands = Arc::new(Mutex::new(Vec::new()));
        let (wake_tx, mut wake_rx) = unbounded();
        let unblock_name = name.clone();
        let worker = spawn_ipc_listener(
            listener,
            move || {
                let _ = windows_pipe::connect(&unblock_name);
            },
            Arc::clone(&pending_files),
            Arc::clone(&pending_ipc_commands),
            wake_tx,
        );

        // A client that connects and never closes must not block later clients.
        let _stuck = windows_pipe::connect_trusted(&name).unwrap();
        forward_files(
            windows_pipe::connect_trusted(&name).unwrap(),
            std::slice::from_ref(&existing),
            true,
        )
        .unwrap();
        assert!(wait_for_wake(&mut wake_rx), "files were not read");
        let mut stream = windows_pipe::connect_trusted(&name).unwrap();
        writeln!(stream, "{}", command_line("new-window")).unwrap();
        drop(stream);
        assert!(wait_for_wake(&mut wake_rx), "command was not read");

        assert_eq!(
            *pending_files.lock(),
            vec![ExternalOpenFile {
                path: std::fs::canonicalize(&existing).unwrap(),
                read_only: true,
            }]
        );
        assert_eq!(*pending_ipc_commands.lock(), vec!["new-window".to_string()]);

        let started = Instant::now();
        drop(worker);
        assert!(
            started.elapsed() < IPC_WORKER_JOIN_TIMEOUT,
            "the wakeup must unblock the pending connect so the worker joins"
        );
    }

    #[test]
    fn test_pipe_listener_refuses_an_existing_pipe() {
        let name = test_pipe_name();
        let _first = PipeListener::bind(&name).unwrap();

        assert!(
            PipeListener::bind(&name).is_err(),
            "a pipe that already exists must not be joined"
        );
    }

    #[test]
    fn test_connect_without_listener_reports_not_found() {
        let error = windows_pipe::connect_trusted(&test_pipe_name()).unwrap_err();

        assert_eq!(error.kind(), ErrorKind::NotFound);
    }

    #[test]
    fn test_pipe_name_is_scoped_to_user_and_session() {
        let name = windows_pipe::pipe_name("0123456789abcdef").unwrap();

        assert!(name.starts_with(r"\\.\pipe\Fulgur-S-1-"), "got {name}");
        assert!(name.ends_with("-0123456789abcdef"), "got {name}");
    }

    #[test]
    fn test_distinct_installations_get_distinct_pipe_names() {
        let base = std::env::temp_dir();
        let installed = super::instance_key::instance_id(&base.join("profile").join("Fulgur"));
        let portable =
            super::instance_key::instance_id(&base.join("stick").join("Fulgur").join("data"));

        assert_ne!(
            windows_pipe::pipe_name(&installed).unwrap(),
            windows_pipe::pipe_name(&portable).unwrap()
        );
    }
}
