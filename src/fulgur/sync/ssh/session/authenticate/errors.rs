use crate::fulgur::sync::ssh::error::SshError;

const LIBSSH2_ERROR_SOCKET_SEND: i32 = -7;
const LIBSSH2_ERROR_TIMEOUT: i32 = -9;
const LIBSSH2_ERROR_SOCKET_DISCONNECT: i32 = -13;
const LIBSSH2_ERROR_AUTHENTICATION_FAILED: i32 = -18;
const LIBSSH2_ERROR_SOCKET_TIMEOUT: i32 = -30;
const LIBSSH2_ERROR_SOCKET_RECV: i32 = -43;

/// Extract the libssh2 session error code from an ssh2 error.
///
/// ### Arguments
/// - `error`: Raw ssh2 error.
///
/// ### Returns
/// - `Some(i32)`: The libssh2 session error code.
/// - `None`: The error is an SFTP error.
fn session_error_code(error: &ssh2::Error) -> Option<i32> {
    match error.code() {
        ssh2::ErrorCode::Session(code) => Some(code),
        ssh2::ErrorCode::SFTP(_) => None,
    }
}

/// Report whether the server explicitly rejected the offered credentials.
///
/// ### Arguments
/// - `error`: Raw ssh2 error returned by an authentication request.
///
/// ### Returns
/// - `true`: The credentials were rejected.
/// - `false`: The request failed for another reason.
pub(super) fn is_authentication_rejection(error: &ssh2::Error) -> bool {
    session_error_code(error) == Some(LIBSSH2_ERROR_AUTHENTICATION_FAILED)
}

/// Report whether an error means the connection itself is broken.
///
/// ### Arguments
/// - `error`: Raw ssh2 error returned by an authentication request.
///
/// ### Returns
/// - `true`: Socket or timeout failure; further authentication attempts are pointless.
/// - `false`: The connection is still usable.
pub(super) fn is_transport_error(error: &ssh2::Error) -> bool {
    matches!(
        session_error_code(error),
        Some(
            LIBSSH2_ERROR_SOCKET_SEND
                | LIBSSH2_ERROR_TIMEOUT
                | LIBSSH2_ERROR_SOCKET_DISCONNECT
                | LIBSSH2_ERROR_SOCKET_TIMEOUT
                | LIBSSH2_ERROR_SOCKET_RECV
        )
    )
}

/// Wrap an authentication request failure as a connection error.
///
/// ### Arguments
/// - `error`: Raw ssh2 error returned by an authentication request.
///
/// ### Returns
/// - `SshError::ConnectionFailed`: Error carrying the libssh2 message.
pub(super) fn auth_request_error(error: &ssh2::Error) -> SshError {
    SshError::ConnectionFailed(format!("SSH authentication request failed: {error}"))
}

#[cfg(test)]
mod tests {
    use super::{auth_request_error, is_authentication_rejection, is_transport_error};
    use crate::fulgur::sync::ssh::error::SshError;

    #[test]
    fn is_authentication_rejection_matches_only_auth_failed() {
        let rejected = ssh2::Error::from_errno(ssh2::ErrorCode::Session(-18));
        let disconnected = ssh2::Error::from_errno(ssh2::ErrorCode::Session(-13));
        assert!(is_authentication_rejection(&rejected));
        assert!(!is_authentication_rejection(&disconnected));
    }

    #[test]
    fn is_transport_error_matches_socket_failures() {
        for code in [-7, -9, -13, -30, -43] {
            let error = ssh2::Error::from_errno(ssh2::ErrorCode::Session(code));
            assert!(
                is_transport_error(&error),
                "code {code} is a transport error"
            );
        }
        for code in [-16, -18, -19] {
            let error = ssh2::Error::from_errno(ssh2::ErrorCode::Session(code));
            assert!(
                !is_transport_error(&error),
                "code {code} is not a transport error"
            );
        }
    }

    #[test]
    fn auth_request_error_maps_to_connection_failed() {
        let error = ssh2::Error::from_errno(ssh2::ErrorCode::Session(-7));
        match auth_request_error(&error) {
            SshError::ConnectionFailed(message) => {
                assert!(message.contains("SSH authentication request failed"));
            }
            other => panic!("expected ConnectionFailed, got {other:?}"),
        }
    }
}
