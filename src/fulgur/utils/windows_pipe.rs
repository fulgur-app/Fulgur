//! Windows named-pipe transport for the single-instance channel.
//!
//! The pipe is private to the user at every level:
//! - its name embeds the user SID and the logon session id, so two accounts (or two
//!   sessions of the same account) never share a channel;
//! - its DACL grants access to the current user only, and remote clients are rejected;
//! - the first instance is created with `FILE_FLAG_FIRST_PIPE_INSTANCE`, so a pipe
//!   pre-created by another process is never joined by the listener;
//! - both ends check that the peer process runs as the same user, so a squatted pipe
//!   is not trusted by a client and a foreign client is dropped by the listener.
//!
//! Connected ends are returned as `std::fs::File`, whose `Read` / `Write` map to
//! `ReadFile` / `WriteFile` and which closes the pipe handle on drop.
#![allow(
    unsafe_code,
    reason = "Win32 named-pipe, token and security-descriptor FFI calls"
)]

use std::{
    fs::{File, OpenOptions},
    io,
    os::windows::{
        fs::OpenOptionsExt,
        io::{AsRawHandle, FromRawHandle, OwnedHandle},
    },
    thread,
    time::{Duration, Instant},
};
use windows::{
    Win32::{
        Foundation::{
            ERROR_NO_DATA, ERROR_PIPE_BUSY, ERROR_PIPE_CONNECTED, HANDLE, HLOCAL, LocalFree,
        },
        Security::{
            Authorization::{
                ConvertSidToStringSidW, ConvertStringSecurityDescriptorToSecurityDescriptorW,
                SDDL_REVISION_1,
            },
            GetTokenInformation, PSECURITY_DESCRIPTOR, SECURITY_ATTRIBUTES, TOKEN_QUERY,
            TOKEN_USER, TokenUser,
        },
        Storage::FileSystem::{
            FILE_FLAG_FIRST_PIPE_INSTANCE, PIPE_ACCESS_INBOUND, SECURITY_IDENTIFICATION,
        },
        System::{
            Pipes::{
                ConnectNamedPipe, CreateNamedPipeW, GetNamedPipeClientProcessId,
                GetNamedPipeServerProcessId, PIPE_READMODE_BYTE, PIPE_REJECT_REMOTE_CLIENTS,
                PIPE_TYPE_BYTE, PIPE_UNLIMITED_INSTANCES, PIPE_WAIT,
            },
            RemoteDesktop::ProcessIdToSessionId,
            Threading::{
                GetCurrentProcess, GetCurrentProcessId, OpenProcess, OpenProcessToken,
                PROCESS_QUERY_LIMITED_INFORMATION,
            },
        },
    },
    core::{HSTRING, PWSTR},
};

/// Size of the inbound buffer of each pipe instance.
const PIPE_BUFFER_SIZE: u32 = 64 * 1024;

/// Maximum time a client waits for a free pipe instance when all are busy.
const CONNECT_BUSY_TIMEOUT: Duration = Duration::from_secs(2);

/// Delay between two connection attempts while all pipe instances are busy.
const CONNECT_BUSY_RETRY: Duration = Duration::from_millis(50);

/// Build the name of the single-instance pipe of the current user, session and installation.
///
/// ### Arguments
/// - `instance_id`: Identifier of the configuration directory, so an installed
///   Fulgur, a portable Fulgur and a `-d` development instance never share a pipe
///
/// ### Errors
/// Returns an `io::Error` if the user SID or the session id cannot be queried.
///
/// ### Returns
/// - `Ok(String)`: The pipe name, e.g. `\\.\pipe\Fulgur-S-1-5-21-...-1-0123456789abcdef`
/// - `Err(io::Error)`: The user SID or the session id could not be queried
pub fn pipe_name(instance_id: &str) -> io::Result<String> {
    let sid = current_user_sid()?;
    let mut session = 0u32;
    // SAFETY: `session` is a valid out-pointer for the duration of the call.
    unsafe { ProcessIdToSessionId(GetCurrentProcessId(), &mut session) }
        .map_err(io::Error::other)?;
    Ok(format!(r"\\.\pipe\Fulgur-{sid}-{session}-{instance_id}"))
}

/// Read the user SID of a process, as a string.
///
/// ### Arguments
/// - `process`: A process handle with `PROCESS_QUERY_LIMITED_INFORMATION` access
///
/// ### Errors
/// Returns an `io::Error` if the process token cannot be opened or queried.
///
/// ### Returns
/// - `Ok(String)`: The SID of the user the process runs as, e.g. `S-1-5-21-...`
/// - `Err(io::Error)`: The token could not be opened or queried
///
/// ### Safety
/// `process` must be a valid process handle (or the current-process pseudo handle).
unsafe fn process_user_sid(process: HANDLE) -> io::Result<String> {
    let mut token = HANDLE::default();
    // SAFETY: `token` is a valid out-pointer; the caller guarantees `process` is valid.
    unsafe { OpenProcessToken(process, TOKEN_QUERY, &mut token) }.map_err(io::Error::other)?;
    // SAFETY: `OpenProcessToken` succeeded, so `token` is an owned, open handle.
    let token = unsafe { OwnedHandle::from_raw_handle(token.0) };
    let token = HANDLE(token.as_raw_handle());

    let mut length = 0u32;
    // SAFETY: a null buffer with a zero length only queries the required size.
    let _ = unsafe { GetTokenInformation(token, TokenUser, None, 0, &mut length) };
    if length == 0 {
        return Err(io::Error::last_os_error());
    }
    // A `u64` buffer keeps `TOKEN_USER` (pointer-aligned) correctly aligned.
    let mut buffer = vec![0u64; (length as usize).div_ceil(size_of::<u64>())];
    // SAFETY: `buffer` holds at least `length` writable bytes.
    unsafe {
        GetTokenInformation(
            token,
            TokenUser,
            Some(buffer.as_mut_ptr().cast()),
            length,
            &mut length,
        )
    }
    .map_err(io::Error::other)?;
    // SAFETY: `GetTokenInformation(TokenUser)` filled `buffer` with a `TOKEN_USER`.
    let user = unsafe { &*buffer.as_ptr().cast::<TOKEN_USER>() };

    let mut sid = PWSTR::null();
    // SAFETY: `user.User.Sid` points into `buffer`, which outlives the call.
    unsafe { ConvertSidToStringSidW(user.User.Sid, &mut sid) }.map_err(io::Error::other)?;
    // SAFETY: on success `sid` is a NUL-terminated string allocated with `LocalAlloc`.
    let result = unsafe { sid.to_string() };
    // SAFETY: `sid` was allocated by `ConvertSidToStringSidW` and is not used afterwards.
    unsafe { LocalFree(Some(HLOCAL(sid.0.cast()))) };
    result.map_err(io::Error::other)
}

/// Read the user SID of the current process.
///
/// ### Errors
/// Returns an `io::Error` if the process token cannot be opened or queried.
///
/// ### Returns
/// - `Ok(String)`: The SID of the current user
/// - `Err(io::Error)`: The token could not be opened or queried
fn current_user_sid() -> io::Result<String> {
    // SAFETY: `GetCurrentProcess` returns a pseudo handle that is always valid.
    unsafe { process_user_sid(GetCurrentProcess()) }
}

/// Check whether a process runs as the current user.
///
/// ### Arguments
/// - `pid`: The id of the process to check
///
/// ### Returns
/// - `true`: The process runs as the current user
/// - `false`: It runs as another user, or its user could not be determined
fn process_is_current_user(pid: u32) -> bool {
    // SAFETY: `OpenProcess` has no pointer arguments.
    let process = match unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) } {
        Ok(process) => process,
        Err(e) => {
            log::warn!("Single-instance: cannot open peer process {pid}: {e}");
            return false;
        }
    };
    // SAFETY: `OpenProcess` succeeded, so `process` is an owned, open handle.
    let process = unsafe { OwnedHandle::from_raw_handle(process.0) };
    // SAFETY: `process` is a valid handle opened with the required access right.
    let peer = unsafe { process_user_sid(HANDLE(process.as_raw_handle())) };
    match (peer, current_user_sid()) {
        (Ok(peer), Ok(current)) => peer == current,
        (Err(e), _) | (_, Err(e)) => {
            log::warn!("Single-instance: cannot read the user of process {pid}: {e}");
            false
        }
    }
}

/// Create one instance of the pipe, accessible to the current user only.
///
/// ### Arguments
/// - `name`: The pipe name
/// - `first`: Whether this must be the first instance; creation then fails if the
///   pipe already exists, whoever created it
///
/// ### Errors
/// Returns an `io::Error` if the security descriptor cannot be built or the pipe
/// instance cannot be created (including when `first` is set and the pipe exists).
///
/// ### Returns
/// - `Ok(File)`: The server end of the new instance, waiting for a client
/// - `Err(io::Error)`: The instance could not be created
fn create_instance(name: &str, first: bool) -> io::Result<File> {
    let sid = current_user_sid()?;
    let attributes_length =
        u32::try_from(size_of::<SECURITY_ATTRIBUTES>()).map_err(io::Error::other)?;
    // Protected DACL with one ACE: generic all for the current user.
    let sddl = HSTRING::from(format!("D:P(A;;GA;;;{sid})"));
    let mut descriptor = PSECURITY_DESCRIPTOR::default();
    // SAFETY: `descriptor` is a valid out-pointer for the duration of the call.
    unsafe {
        ConvertStringSecurityDescriptorToSecurityDescriptorW(
            &sddl,
            SDDL_REVISION_1,
            &mut descriptor,
            None,
        )
    }
    .map_err(io::Error::other)?;
    let attributes = SECURITY_ATTRIBUTES {
        nLength: attributes_length,
        lpSecurityDescriptor: descriptor.0,
        bInheritHandle: false.into(),
    };
    let open_mode = if first {
        PIPE_ACCESS_INBOUND | FILE_FLAG_FIRST_PIPE_INSTANCE
    } else {
        PIPE_ACCESS_INBOUND
    };
    // SAFETY: `attributes` and its descriptor stay alive for the duration of the call.
    let handle = unsafe {
        CreateNamedPipeW(
            &HSTRING::from(name),
            open_mode,
            PIPE_TYPE_BYTE | PIPE_READMODE_BYTE | PIPE_WAIT | PIPE_REJECT_REMOTE_CLIENTS,
            PIPE_UNLIMITED_INSTANCES,
            0,
            PIPE_BUFFER_SIZE,
            0,
            Some(&attributes),
        )
    };
    let create_error = io::Error::last_os_error();
    // SAFETY: the descriptor was allocated by the conversion above; the kernel has
    // copied it into the pipe object, so it is no longer needed.
    unsafe { LocalFree(Some(HLOCAL(descriptor.0))) };
    if handle.is_invalid() {
        return Err(create_error);
    }
    // SAFETY: `CreateNamedPipeW` succeeded, so `handle` is an owned, open handle.
    Ok(unsafe { File::from_raw_handle(handle.0) })
}

/// Server side of the single-instance pipe.
///
/// Always holds one instance waiting for a client. Each accepted client gets
/// its own instance, so a slow or stuck client never blocks the next one.
pub struct PipeListener {
    /// The pipe name
    name: String,
    /// The instance waiting for the next client
    pending: File,
}

impl PipeListener {
    /// Create the pipe, failing if it already exists.
    ///
    /// ### Arguments
    /// - `name`: The pipe name
    ///
    /// ### Errors
    /// Returns an `io::Error` if the pipe already exists (another instance is
    /// listening, or another process squats the name) or cannot be created.
    ///
    /// ### Returns
    /// - `Ok(PipeListener)`: The listener, ready to accept clients
    /// - `Err(io::Error)`: The pipe could not be created
    pub fn bind(name: &str) -> io::Result<Self> {
        Ok(Self {
            name: name.to_string(),
            pending: create_instance(name, true)?,
        })
    }

    /// Wait for the next client and return its connection.
    ///
    /// ### Errors
    /// Returns an `io::Error` if waiting for a client fails, the next pipe
    /// instance cannot be created, or the client does not run as the current
    /// user (its connection is then closed).
    ///
    /// ### Returns
    /// - `Ok(File)`: The connection to a client running as the current user
    /// - `Err(io::Error)`: No trusted client connection could be obtained
    pub fn accept(&mut self) -> io::Result<File> {
        // SAFETY: `pending` owns an open pipe handle; the call is synchronous.
        match unsafe { ConnectNamedPipe(HANDLE(self.pending.as_raw_handle()), None) } {
            Ok(()) => {}
            // The client connected (or connected and already closed) between the
            // instance creation and this call: its data is still readable.
            Err(e)
                if e.code() == ERROR_PIPE_CONNECTED.to_hresult()
                    || e.code() == ERROR_NO_DATA.to_hresult() => {}
            Err(e) => return Err(io::Error::other(e)),
        }
        let next = create_instance(&self.name, false)?;
        let connection = std::mem::replace(&mut self.pending, next);

        let mut pid = 0u32;
        // SAFETY: `connection` owns a connected pipe handle; `pid` is a valid out-pointer.
        unsafe { GetNamedPipeClientProcessId(HANDLE(connection.as_raw_handle()), &mut pid) }
            .map_err(io::Error::other)?;
        if !process_is_current_user(pid) {
            return Err(io::Error::new(
                io::ErrorKind::PermissionDenied,
                format!("rejected client process {pid} not running as the current user"),
            ));
        }
        Ok(connection)
    }
}

/// Open the client end of the pipe, waiting briefly while all instances are busy.
///
/// The server cannot impersonate the client beyond identification level.
///
/// ### Arguments
/// - `name`: The pipe name
///
/// ### Errors
/// Returns an `io::Error` if the pipe does not exist, stays busy past the
/// timeout, or cannot be opened.
///
/// ### Returns
/// - `Ok(File)`: The client end of the pipe, open for writing
/// - `Err(io::Error)`: The pipe could not be opened
pub fn connect(name: &str) -> io::Result<File> {
    let deadline = Instant::now() + CONNECT_BUSY_TIMEOUT;
    loop {
        match OpenOptions::new()
            .write(true)
            .security_qos_flags(SECURITY_IDENTIFICATION.0)
            .open(name)
        {
            Err(e)
                if e.raw_os_error() == Some(ERROR_PIPE_BUSY.0.cast_signed())
                    && Instant::now() < deadline =>
            {
                thread::sleep(CONNECT_BUSY_RETRY);
            }
            result => return result,
        }
    }
}

/// Open the client end of the pipe and check that its server runs as the current user.
///
/// ### Arguments
/// - `name`: The pipe name
///
/// ### Errors
/// Returns an `io::Error` if the pipe cannot be opened, or with
/// `PermissionDenied` if the server is not a process of the current user
/// (e.g. another account created the pipe first).
///
/// ### Returns
/// - `Ok(File)`: A connection to a server running as the current user
/// - `Err(io::Error)`: No trusted server is listening
pub fn connect_trusted(name: &str) -> io::Result<File> {
    let connection = connect(name)?;
    let mut pid = 0u32;
    // SAFETY: `connection` owns a connected pipe handle; `pid` is a valid out-pointer.
    unsafe { GetNamedPipeServerProcessId(HANDLE(connection.as_raw_handle()), &mut pid) }
        .map_err(io::Error::other)?;
    if !process_is_current_user(pid) {
        log::warn!("Single-instance: ignoring pipe served by process {pid} of another user");
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("pipe server process {pid} does not run as the current user"),
        ));
    }
    Ok(connection)
}
