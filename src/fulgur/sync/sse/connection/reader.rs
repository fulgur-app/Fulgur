use crate::fulgur::sync::share::MAX_SYNC_SHARE_PAYLOAD_BYTES;
use std::{
    io::{BufReader, Read},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
};

/// Maximum size for SSE event data accumulation (10x payload limit to account for
/// base64 encoding overhead and JSON wrapper)
pub(super) const MAX_SSE_EVENT_DATA_BYTES: usize = MAX_SYNC_SHARE_PAYLOAD_BYTES * 10;

/// Error type for line reading with shutdown support
pub(super) enum ReadError {
    /// I/O error during reading
    Io(std::io::Error),
    /// Shutdown was requested
    Shutdown,
}

/// Read a line from a buffered reader, checking for shutdown between reads.
///
/// ### Arguments
/// - `reader`: The buffered reader to read from
/// - `shutdown_flag`: Atomic flag to check for shutdown requests
///
/// ### Returns
/// - `Ok(Some(String))`: A line was read successfully
/// - `Ok(None)`: End of stream reached
/// - `Err(ReadError::Shutdown)`: Shutdown was requested
/// - `Err(ReadError::Io)`: I/O error occurred, the agent timeout fired, or a
///   single line exceeded `MAX_SSE_EVENT_DATA_BYTES` (forcing a reconnect)
pub(super) fn read_sse_line<R: Read>(
    reader: &mut BufReader<R>,
    shutdown_flag: &Arc<AtomicBool>,
) -> Result<Option<String>, ReadError> {
    let mut line = Vec::new();
    let mut byte = [0u8; 1];

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
            Err(e) if e.kind() == std::io::ErrorKind::Interrupted => {}
            Err(e) => {
                return Err(ReadError::Io(e));
            }
        }
    }
}
