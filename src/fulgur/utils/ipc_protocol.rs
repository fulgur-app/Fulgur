//! Line protocol spoken between Fulgur instances over the single-instance channel.
//!
//! Each message is one line. A `CMD:` prefix carries a command (`CMD:new-tab`),
//! anything else is a file path, prefixed with `RO:` when the file must open
//! read-only. The transport lives in `single_instance`; this module is
//! platform-neutral so its tests run on every OS.

use crate::fulgur::shared_state::ExternalOpenFile;
use parking_lot::Mutex;
use std::{
    io::BufRead,
    path::{Path, PathBuf},
};

/// Prefix used to distinguish command messages from file-path messages.
const CMD_PREFIX: &str = "CMD:";

/// Prefix marking a forwarded file path as read-only.
const READ_ONLY_PREFIX: &str = "RO:";

/// A message received from another Fulgur instance.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcMessage {
    /// An in-process action such as `new-tab` or `new-window`
    Command(String),
    /// A file to open, with its read-only marker
    File(ExternalOpenFile),
}

/// Build the IPC line forwarding a file to the running instance.
///
/// ### Arguments
/// - `path`: The absolute path of the file
/// - `read_only`: Whether the file must open read-only
///
/// ### Returns
/// - `String`: The line to write, without its line terminator
#[must_use]
pub fn file_line(path: &Path, read_only: bool) -> String {
    let prefix = if read_only { READ_ONLY_PREFIX } else { "" };
    format!("{prefix}{}", path.display())
}

/// Build the IPC line sending a command to the running instance.
///
/// ### Arguments
/// - `cmd`: The command identifier (e.g. `"new-tab"`)
///
/// ### Returns
/// - `String`: The line to write, without its line terminator
#[must_use]
pub fn command_line(cmd: &str) -> String {
    format!("{CMD_PREFIX}{cmd}")
}

/// Parse an IPC line forwarding a file from another instance.
///
/// ### Arguments
/// - `line`: The received line, which is not a `CMD:` command
///
/// ### Returns
/// - `ExternalOpenFile`: The forwarded path and its read-only marker
fn parse_file_line(line: &str) -> ExternalOpenFile {
    match line.strip_prefix(READ_ONLY_PREFIX) {
        Some(path) => ExternalOpenFile {
            path: PathBuf::from(path),
            read_only: true,
        },
        None => ExternalOpenFile {
            path: PathBuf::from(line),
            read_only: false,
        },
    }
}

/// Parse one line received from another instance.
///
/// ### Arguments
/// - `line`: The received line, without its line terminator
///
/// ### Returns
/// - `Some(IpcMessage)`: The command or file the line carries
/// - `None`: The line is empty
#[must_use]
pub fn parse_ipc_line(line: &str) -> Option<IpcMessage> {
    if line.is_empty() {
        return None;
    }
    Some(match line.strip_prefix(CMD_PREFIX) {
        Some(cmd) => IpcMessage::Command(cmd.to_string()),
        None => IpcMessage::File(parse_file_line(line)),
    })
}

/// Read every message of one connection and queue it for delivery.
///
/// Commands are appended to `pending_ipc_commands`. Files are appended to
/// `pending_files` only if they still exist, since the sender may have
/// forwarded a path that was deleted in the meantime.
///
/// ### Arguments
/// - `reader`: The connection, read line by line until it closes or fails
/// - `pending_files`: Queue receiving the forwarded files
/// - `pending_ipc_commands`: Queue receiving the forwarded commands
pub fn read_ipc_connection(
    reader: impl BufRead,
    pending_files: &Mutex<Vec<ExternalOpenFile>>,
    pending_ipc_commands: &Mutex<Vec<String>>,
) {
    for line in reader.lines().map_while(Result::ok) {
        match parse_ipc_line(&line) {
            Some(IpcMessage::Command(cmd)) => {
                log::info!("IPC: received command '{cmd}'");
                pending_ipc_commands.lock().push(cmd);
            }
            Some(IpcMessage::File(file)) if file.path.exists() => {
                log::info!(
                    "IPC: queuing file from another instance: {}",
                    file.path.display()
                );
                pending_files.lock().push(file);
            }
            Some(IpcMessage::File(file)) => {
                log::debug!(
                    "IPC: dropping forwarded file that no longer exists: {}",
                    file.path.display()
                );
            }
            None => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{IpcMessage, command_line, file_line, parse_ipc_line, read_ipc_connection};
    use crate::fulgur::shared_state::ExternalOpenFile;
    use parking_lot::Mutex;
    use std::{
        io::Cursor,
        path::{Path, PathBuf},
    };

    #[test]
    fn test_file_line_round_trips_a_read_only_file() {
        let message = parse_ipc_line(&file_line(Path::new("/tmp/a.txt"), true));
        assert_eq!(
            message,
            Some(IpcMessage::File(ExternalOpenFile {
                path: PathBuf::from("/tmp/a.txt"),
                read_only: true,
            }))
        );
    }

    #[test]
    fn test_file_line_round_trips_an_editable_file() {
        let message = parse_ipc_line(&file_line(Path::new("/tmp/a.txt"), false));
        assert_eq!(
            message,
            Some(IpcMessage::File(ExternalOpenFile {
                path: PathBuf::from("/tmp/a.txt"),
                read_only: false,
            }))
        );
    }

    #[test]
    fn test_command_line_round_trips() {
        assert_eq!(
            parse_ipc_line(&command_line("new-window")),
            Some(IpcMessage::Command("new-window".to_string()))
        );
    }

    #[test]
    fn test_parse_ipc_line_ignores_an_empty_line() {
        assert_eq!(parse_ipc_line(""), None);
    }

    #[test]
    fn test_read_ipc_connection_routes_commands_and_drops_missing_files() {
        let dir = tempfile::tempdir().unwrap();
        let editable = dir.path().join("editable.txt");
        let read_only = dir.path().join("read_only.txt");
        let missing = dir.path().join("missing.txt");
        std::fs::write(&editable, "a").unwrap();
        std::fs::write(&read_only, "b").unwrap();
        let input = [
            file_line(&editable, false),
            String::new(),
            file_line(&missing, false),
            command_line("new-tab"),
            file_line(&read_only, true),
        ]
        .join("\n");
        let pending_files = Mutex::new(Vec::new());
        let pending_ipc_commands = Mutex::new(Vec::new());

        read_ipc_connection(Cursor::new(input), &pending_files, &pending_ipc_commands);

        assert_eq!(
            *pending_files.lock(),
            vec![
                ExternalOpenFile {
                    path: editable,
                    read_only: false,
                },
                ExternalOpenFile {
                    path: read_only,
                    read_only: true,
                },
            ]
        );
        assert_eq!(*pending_ipc_commands.lock(), vec!["new-tab".to_string()]);
    }
}
