//! Single-instance coordination for Windows and Linux.
//!
//! When a new Fulgur process is launched with a file-path argument (taskbar
//! jump list on Windows, double-click / "Open with" on Linux), this module
//! detects the already-running instance, forwards the path to it, and lets the
//! caller exit early. Windows uses a loopback TCP connection; Linux uses a
//! Unix domain socket in the configuration directory, so the channel is private
//! to the user and a dev instance started with `-d` stays isolated.
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
use std::net::{TcpListener as IpcListener, TcpStream as IpcStream};
#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixListener as IpcListener, UnixStream as IpcStream};
#[cfg(target_os = "linux")]
use std::path::Path;
use std::{
    io::{self, BufReader, Write},
    path::PathBuf,
    sync::{Arc, atomic::Ordering},
    time::Duration,
};

/// Loopback port used for Fulgur IPC. Chosen to be unlikely to conflict.
#[cfg(target_os = "windows")]
const IPC_PORT: u16 = 29764;

/// File name of the Unix domain socket, created in the configuration directory.
#[cfg(target_os = "linux")]
const IPC_SOCKET_NAME: &str = "fulgur.sock";

/// Maximum time a dropped IPC listener worker is joined before being detached.
/// The wakeup self-connect unblocks `accept` immediately, so this is short.
const IPC_WORKER_JOIN_TIMEOUT: Duration = Duration::from_secs(2);

/// Resolve the path of the Unix domain socket used for IPC.
///
/// ### Returns
/// - `Ok(PathBuf)`: The socket path inside the configuration directory
/// - `Err(io::Error)`: The configuration directory could not be resolved
#[cfg(target_os = "linux")]
fn socket_path() -> io::Result<PathBuf> {
    crate::fulgur::utils::paths::config_dir()
        .map(|dir| dir.join(IPC_SOCKET_NAME))
        .map_err(io::Error::other)
}

/// Connect to the IPC listener of the running instance.
///
/// ### Returns
/// - `Ok(IpcStream)`: A connection to the running instance
/// - `Err(io::Error)`: No instance is listening
fn connect() -> io::Result<IpcStream> {
    #[cfg(target_os = "windows")]
    return IpcStream::connect(("127.0.0.1", IPC_PORT));
    #[cfg(target_os = "linux")]
    return IpcStream::connect(socket_path()?);
}

/// Bind the IPC listener of this instance.
///
/// ### Returns
/// - `Ok(IpcListener)`: The bound listener
/// - `Err(io::Error)`: The listener could not be bound
fn bind() -> io::Result<IpcListener> {
    #[cfg(target_os = "windows")]
    return IpcListener::bind(("127.0.0.1", IPC_PORT));
    #[cfg(target_os = "linux")]
    return bind_socket(&socket_path()?);
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
fn forward_files(mut stream: impl Write, paths: &[PathBuf], read_only: bool) {
    for path in paths {
        let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
        let _ = writeln!(stream, "{}", file_line(&path, read_only));
    }
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
/// - `false`: No existing instance is running - caller should continue
#[must_use]
pub fn try_forward_to_existing_instance(paths: &[PathBuf], read_only: bool) -> bool {
    match connect() {
        Ok(stream) => {
            forward_files(stream, paths, read_only);
            log::info!(
                "Single-instance: forwarded {} path(s) to running instance",
                paths.len()
            );
            true
        }
        Err(_) => false,
    }
}

/// Try to send a command to an already-running Fulgur instance.
///
/// Writes a single `CMD:<cmd>` line (e.g. `CMD:new-tab`) to the loopback
/// listener. If the connection succeeds the command has been delivered and
/// the caller should exit immediately. If the connection is refused there is
/// no existing instance and the caller should start normally.
///
/// ### Arguments
/// - `cmd`: The command identifier to send (e.g. `"new-tab"`, `"new-window"`)
///
/// ### Returns
/// - `true`: Another instance was found and the command was forwarded - caller should exit
/// - `false`: No existing instance is running - caller should continue
#[must_use]
pub fn try_send_command_to_existing_instance(cmd: &str) -> bool {
    match connect() {
        Ok(mut stream) => {
            let _ = writeln!(stream, "{}", command_line(cmd));
            log::info!("Single-instance: forwarded command '{cmd}' to running instance");
            true
        }
        Err(_) => false,
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
    let unblock_accept = || {
        let _ = connect();
    };
    Some(spawn_ipc_listener(
        listener,
        unblock_accept,
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
    listener: IpcListener,
    unblock_accept: impl Fn() + Send + Sync + 'static,
    pending_files: Arc<Mutex<Vec<ExternalOpenFile>>>,
    pending_ipc_commands: Arc<Mutex<Vec<String>>>,
    wake: UnboundedSender<()>,
) -> Worker {
    Worker::spawn(
        "fulgur-ipc-listener",
        IPC_WORKER_JOIN_TIMEOUT,
        move |shutdown| {
            for stream in listener.incoming() {
                if shutdown.load(Ordering::Relaxed) {
                    log::info!("IPC listener shutdown requested, stopping");
                    break;
                }
                match stream {
                    Ok(stream) => {
                        read_ipc_connection(
                            BufReader::new(stream),
                            &pending_files,
                            &pending_ipc_commands,
                        );
                        let _ = wake.unbounded_send(());
                    }
                    Err(e) => {
                        log::warn!("IPC listener accept error: {e}");
                    }
                }
            }
        },
    )
    .with_wakeup(unblock_accept)
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
        );
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
