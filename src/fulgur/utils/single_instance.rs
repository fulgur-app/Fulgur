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
//! focuses the file in the last focused window - the same queue used by the
//! macOS "Open With" handler.
//!
//! Jump list Tasks ("New Tab", "New Window") send a `CMD:new-tab` /
//! `CMD:new-window` line instead of a file path. The listener pushes these into
//! `pending_ipc_commands` and the render loop dispatches them in-process.

use crate::fulgur::utils::worker::Worker;
use futures::channel::mpsc::UnboundedSender;
use parking_lot::Mutex;
#[cfg(target_os = "windows")]
use std::net::{TcpListener as IpcListener, TcpStream as IpcStream};
#[cfg(target_os = "linux")]
use std::os::unix::net::{UnixListener as IpcListener, UnixStream as IpcStream};
use std::{
    io::{self, BufRead, BufReader, Write},
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

/// Prefix used to distinguish command messages from file-path messages.
const CMD_PREFIX: &str = "CMD:";

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
/// On Linux a socket file left behind by a crashed instance is removed and the
/// bind retried, while a socket that still accepts connections is left alone.
///
/// ### Returns
/// - `Ok(IpcListener)`: The bound listener
/// - `Err(io::Error)`: The listener could not be bound
fn bind() -> io::Result<IpcListener> {
    #[cfg(target_os = "windows")]
    return IpcListener::bind(("127.0.0.1", IPC_PORT));
    #[cfg(target_os = "linux")]
    {
        let path = socket_path()?;
        match IpcListener::bind(&path) {
            Err(e)
                if e.kind() == io::ErrorKind::AddrInUse && IpcStream::connect(&path).is_err() =>
            {
                log::info!("Removing stale single-instance socket: {}", path.display());
                std::fs::remove_file(&path)?;
                IpcListener::bind(&path)
            }
            result => result,
        }
    }
}

/// Try to forward `paths` to an already-running Fulgur instance.
///
/// Connects to the listener started by the primary instance. Paths are made
/// absolute first, since the running instance has its own working directory.
/// If the connection succeeds the paths are written one per line and the
/// caller should exit immediately. If the connection is refused this is
/// the first instance and the caller should continue normally.
///
/// ### Arguments
/// - `paths`: The file paths to forward to the running instance
///
/// ### Returns
/// - `true`: Another instance was found and the paths were forwarded - caller should exit
/// - `false`: No existing instance is running - caller should continue
#[must_use]
pub fn try_forward_to_existing_instance(paths: &[PathBuf]) -> bool {
    match connect() {
        Ok(mut stream) => {
            for path in paths {
                let path = std::fs::canonicalize(path).unwrap_or_else(|_| path.clone());
                let _ = writeln!(stream, "{}", path.display());
            }
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
            let _ = writeln!(stream, "{CMD_PREFIX}{cmd}");
            log::info!("Single-instance: forwarded command '{cmd}' to running instance");
            true
        }
        Err(_) => false,
    }
}

/// Spawn a background thread that listens for messages from new Fulgur processes.
///
/// File-path lines are appended to `pending_files` so the render cycle can
/// open them, mirroring the macOS "Open With" path. Lines prefixed with
/// `CMD:` are appended to `pending_ipc_commands` so the render cycle can
/// dispatch in-process actions such as opening a new tab or window. After each
/// connection is read, a message is sent on `wake` so the app processes the
/// queues right away instead of waiting for a window to re-render.
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
    pending_files: Arc<Mutex<Vec<PathBuf>>>,
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

    let worker = Worker::spawn(
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
                        let reader = BufReader::new(stream);
                        for line in reader.lines().map_while(Result::ok) {
                            if line.is_empty() {
                                continue;
                            }
                            if let Some(cmd) = line.strip_prefix(CMD_PREFIX) {
                                log::info!("IPC: received command '{cmd}'");
                                pending_ipc_commands.lock().push(cmd.to_string());
                            } else {
                                let path = PathBuf::from(&line);
                                if path.exists() {
                                    log::info!(
                                        "IPC: queuing file from another instance: {}",
                                        path.display()
                                    );
                                    pending_files.lock().push(path);
                                }
                            }
                        }
                        let _ = wake.unbounded_send(());
                    }
                    Err(e) => {
                        log::warn!("IPC listener accept error: {e}");
                    }
                }
            }
        },
    )
    .with_wakeup(|| {
        // Unblock the pending accept so the loop observes the shutdown flag.
        let _ = connect();
    });
    Some(worker)
}
