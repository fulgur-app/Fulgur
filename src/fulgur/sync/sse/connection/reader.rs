use crate::fulgur::sync::share::MAX_SYNC_SHARE_PAYLOAD_BYTES;
use std::{
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::{Duration, Instant},
};

/// Maximum size for SSE event data accumulation (10x payload limit to account for
/// base64 encoding overhead and JSON wrapper)
pub(super) const MAX_SSE_EVENT_DATA_BYTES: usize = MAX_SYNC_SHARE_PAYLOAD_BYTES * 10;

/// Absolute deadline for receiving any byte on the SSE stream before the
/// connection is considered dead and an error is returned so the caller
/// reconnects
const SSE_READ_DEADLINE: Duration = Duration::from_mins(1);

/// Error type for line reading with shutdown support
pub(super) enum ReadError {
    /// I/O error during reading
    Io(std::io::Error),
    /// Shutdown was requested
    Shutdown,
}

/// Read a line from a buffered reader with periodic shutdown checks and an absolute read deadline.
///
/// ### Arguments
/// - `reader`: The buffered reader to read from
/// - `shutdown_flag`: Atomic flag to check for shutdown requests
///
/// ### Returns
/// - `Ok(Some(String))`: A line was read successfully
/// - `Ok(None)`: End of stream reached
/// - `Err(ReadError::Shutdown)`: Shutdown was requested
/// - `Err(ReadError::Io)`: I/O error occurred, the read deadline elapsed, or a
///   single line exceeded `MAX_SSE_EVENT_DATA_BYTES` (forcing a reconnect)
pub(super) fn read_line_with_timeout<R: Read>(
    reader: &mut BufReader<R>,
    shutdown_flag: &Arc<AtomicBool>,
) -> Result<Option<String>, ReadError> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];
    let mut last_byte_received = Instant::now();

    loop {
        if shutdown_flag.load(Ordering::Relaxed) {
            return Err(ReadError::Shutdown);
        }
        match reader.read(&mut byte) {
            Ok(0) => {
                if line.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
            }
            Ok(_) => {
                last_byte_received = Instant::now();
                if byte[0] == b'\n' {
                    if line.last() == Some(&b'\r') {
                        line.pop();
                    }
                    return Ok(Some(String::from_utf8_lossy(&line).into_owned()));
                }
                if line.len() >= MAX_SSE_EVENT_DATA_BYTES {
                    return Err(ReadError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!(
                            "SSE line exceeds size limit ({MAX_SSE_EVENT_DATA_BYTES} bytes), connection presumed malicious"
                        ),
                    )));
                }
                line.push(byte[0]);
            }
            Err(e)
                if e.kind() == std::io::ErrorKind::WouldBlock
                    || e.kind() == std::io::ErrorKind::TimedOut
                    || e.kind() == std::io::ErrorKind::Interrupted =>
            {
                if last_byte_received.elapsed() > SSE_READ_DEADLINE {
                    return Err(ReadError::Io(std::io::Error::new(
                        std::io::ErrorKind::TimedOut,
                        format!(
                            "no data received within {}s, connection presumed dead",
                            SSE_READ_DEADLINE.as_secs()
                        ),
                    )));
                }
                thread::sleep(Duration::from_millis(10));
            }
            Err(e) => {
                return Err(ReadError::Io(e));
            }
        }
    }
}
