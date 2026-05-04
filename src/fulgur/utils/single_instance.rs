//! Windows single-instance coordination via TCP loopback.
//!
//! When the taskbar jump list launches a new Fulgur process with a file-path
//! argument, this module detects the already-running instance, forwards the
//! path to it over a loopback TCP connection, and lets the caller exit early.
//!
//! The listening instance receives the path, appends it to the shared
//! `pending_files` queue, and the next render frame opens / focuses the file -
//! the same mechanism used by the macOS "Open With" handler.
//!
//! Jump list Tasks ("New Tab", "New Window") send a `CMD:new-tab` /
//! `CMD:new-window` line instead of a file path. The listener pushes these into
//! `pending_ipc_commands` and the render loop dispatches them in-process.

use parking_lot::Mutex;
use std::{
    io::{BufRead, BufReader, Write},
    net::{TcpListener, TcpStream},
    path::PathBuf,
    sync::Arc,
};

/// Loopback port used for Fulgur IPC. Chosen to be unlikely to conflict.
const IPC_PORT: u16 = 29764;

/// Prefix used to distinguish command messages from file-path messages.
const CMD_PREFIX: &str = "CMD:";

/// Try to forward `paths` to an already-running Fulgur instance.
///
/// Connects to the loopback listener started by the primary instance.
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
pub fn try_forward_to_existing_instance(paths: &[PathBuf]) -> bool {
    match TcpStream::connect(("127.0.0.1", IPC_PORT)) {
        Ok(mut stream) => {
            for path in paths {
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
pub fn try_send_command_to_existing_instance(cmd: &str) -> bool {
    match TcpStream::connect(("127.0.0.1", IPC_PORT)) {
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
/// dispatch in-process actions such as opening a new tab or window.
///
/// ### Arguments
/// - `pending_files`: Shared queue to receive file paths forwarded by other instances
/// - `pending_ipc_commands`: Shared queue to receive command strings forwarded by other instances
pub fn start_ipc_listener(
    pending_files: Arc<Mutex<Vec<PathBuf>>>,
    pending_ipc_commands: Arc<Mutex<Vec<String>>>,
) {
    let listener = match TcpListener::bind(("127.0.0.1", IPC_PORT)) {
        Ok(l) => l,
        Err(e) => {
            log::warn!("Could not start single-instance IPC listener: {e}");
            return;
        }
    };

    std::thread::spawn(move || {
        for stream in listener.incoming() {
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
                                log::info!("IPC: queuing file from jump list: {}", path.display());
                                pending_files.lock().push(path);
                            }
                        }
                    }
                }
                Err(e) => {
                    log::warn!("IPC listener accept error: {e}");
                }
            }
        }
    });
}
