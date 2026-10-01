use super::MAX_PROMPT_ATTEMPTS;
use super::errors::{auth_request_error, is_authentication_rejection, is_transport_error};
use crate::fulgur::sync::ssh::auth::{CredentialPrompter, PassphraseAnswer, SshAuth};
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::identities::private_key_is_encrypted;
use ssh2::Session;
use std::path::{Path, PathBuf};

/// Outcome of a single public-key authentication request.
pub(super) enum KeyAttempt {
    Accepted,
    /// The server does not accept this key for the user.
    Rejected,
    /// The key could not be used locally: wrong or missing passphrase, or unreadable file.
    Unusable,
}

/// Outcome of authenticating with one key file, passphrase prompts included.
pub(super) enum KeyOutcome {
    Authenticated(SshAuth),
    Rejected,
    Unusable,
    Skipped,
}

/// Try the identities held by the running ssh-agent.
///
/// `Session::userauth_agent` only tries the first identity, so identities are iterated here.
///
/// ### Arguments
/// - `session`: Session to authenticate.
/// - `user`: Remote username.
/// - `allowed_blobs`: With `IdentitiesOnly`, the public key blobs of the configured
///   identities; only matching agent identities are offered. `None` offers them all.
///
/// ### Errors
/// Returns `SshError::ConnectionFailed` on transport failures.
///
/// ### Returns
/// - `Ok(true)`: An agent identity authenticated the session.
/// - `Ok(false)`: No agent is running, it holds no identities, or all were rejected.
/// - `Err(SshError)`: The connection failed.
pub(super) fn try_agent(
    session: &Session,
    user: &str,
    allowed_blobs: Option<&[Vec<u8>]>,
) -> Result<bool, SshError> {
    let mut agent = match session.agent() {
        Ok(agent) => agent,
        Err(error) => {
            log::debug!("ssh-agent unavailable: {error}");
            return Ok(false);
        }
    };
    if let Err(error) = agent.connect().and_then(|()| agent.list_identities()) {
        log::debug!("ssh-agent unavailable: {error}");
        return Ok(false);
    }
    let identities = agent.identities().unwrap_or_default();
    let offered = identities.iter().filter(|identity| {
        allowed_blobs.is_none_or(|blobs| blobs.iter().any(|blob| blob == identity.blob()))
    });
    let mut outcome = Ok(false);
    for identity in offered {
        match agent.userauth(user, identity) {
            Ok(()) => {
                outcome = Ok(session.authenticated());
                break;
            }
            Err(error) if is_transport_error(&error) => {
                outcome = Err(auth_request_error(&error));
                break;
            }
            Err(error) => log::debug!(
                "ssh-agent identity '{}' was not accepted: {error}",
                identity.comment()
            ),
        }
    }
    let _ = agent.disconnect();
    outcome
}

/// Authenticate with a private key file, prompting for its passphrase when needed.
///
/// The key is first offered without a passphrase. libssh2 asks the server whether it
/// accepts the public key before signing, so a server rejection is detected without
/// prompting, and the passphrase is only requested for keys the server would accept
/// (or when the public key cannot be read without it).
///
/// ### Arguments
/// - `session`: Session to authenticate.
/// - `user`: Remote username.
/// - `path`: Private key file.
/// - `prompter`: Source of the passphrase.
///
/// ### Errors
/// Returns `SshError::AuthCancelled` when the user cancels the passphrase prompt and
/// `SshError::ConnectionFailed` on transport failures.
///
/// ### Returns
/// - `Ok(KeyOutcome::Authenticated)`: The key authenticated the session.
/// - `Ok(KeyOutcome::Rejected)`: The server does not accept this key.
/// - `Ok(KeyOutcome::Unusable)`: The key could not be loaded, or no correct passphrase was given.
/// - `Ok(KeyOutcome::Skipped)`: The user chose not to unlock this key.
/// - `Err(SshError)`: The user cancelled or the connection failed.
pub(super) fn authenticate_with_key_file(
    session: &Session,
    user: &str,
    path: &Path,
    prompter: &mut dyn CredentialPrompter,
) -> Result<KeyOutcome, SshError> {
    match try_key_file(session, user, path, None)? {
        KeyAttempt::Accepted => {
            return Ok(KeyOutcome::Authenticated(SshAuth::KeyFile {
                path: path.to_path_buf(),
                passphrase: None,
            }));
        }
        KeyAttempt::Rejected => return Ok(KeyOutcome::Rejected),
        KeyAttempt::Unusable if !private_key_is_encrypted(path) => {
            return Ok(KeyOutcome::Unusable);
        }
        KeyAttempt::Unusable => {}
    }

    let mut error = None;
    for _ in 0..MAX_PROMPT_ATTEMPTS {
        let passphrase = match prompter.passphrase(path, error.take()) {
            PassphraseAnswer::Passphrase(passphrase) => passphrase,
            PassphraseAnswer::Skip => return Ok(KeyOutcome::Skipped),
            PassphraseAnswer::Cancel => return Err(SshError::AuthCancelled),
        };
        match try_key_file(session, user, path, Some(passphrase.as_str()))? {
            KeyAttempt::Accepted => {
                return Ok(KeyOutcome::Authenticated(SshAuth::KeyFile {
                    path: path.to_path_buf(),
                    passphrase: Some(passphrase),
                }));
            }
            KeyAttempt::Rejected => return Ok(KeyOutcome::Rejected),
            KeyAttempt::Unusable => error = Some("Wrong passphrase, try again.".to_string()),
        }
    }
    Ok(KeyOutcome::Unusable)
}

/// Send one public-key authentication request.
///
/// ### Arguments
/// - `session`: Session to authenticate.
/// - `user`: Remote username.
/// - `private_key`: Private key file.
/// - `passphrase`: Passphrase for an encrypted key, or `None` to try without one.
///
/// ### Errors
/// Returns `SshError::ConnectionFailed` on transport failures.
///
/// ### Returns
/// - `Ok(KeyAttempt)`: How the server or the local key loading responded.
/// - `Err(SshError)`: The connection failed.
pub(super) fn try_key_file(
    session: &Session,
    user: &str,
    private_key: &Path,
    passphrase: Option<&str>,
) -> Result<KeyAttempt, SshError> {
    let public_key = public_key_path(private_key);
    // Never pass a NULL passphrase: libssh2's OpenSSL callback reads it unconditionally
    // for encrypted PEM keys.
    let passphrase = passphrase.unwrap_or("");
    match session.userauth_pubkey_file(user, public_key.as_deref(), private_key, Some(passphrase)) {
        Ok(()) if session.authenticated() => Ok(KeyAttempt::Accepted),
        Ok(()) => Ok(KeyAttempt::Rejected),
        Err(error) if is_transport_error(&error) => Err(auth_request_error(&error)),
        Err(error) if is_authentication_rejection(&error) => Ok(KeyAttempt::Rejected),
        Err(error) => {
            log::debug!("Cannot use SSH key {}: {error}", private_key.display());
            Ok(KeyAttempt::Unusable)
        }
    }
}

/// Locate the `.pub` file next to a private key.
///
/// ### Arguments
/// - `private_key`: Private key file.
///
/// ### Returns
/// - `Some(PathBuf)`: `<private_key>.pub` exists.
/// - `None`: No public key file; libssh2 then derives it from the private key.
fn public_key_path(private_key: &Path) -> Option<PathBuf> {
    let mut public_key = private_key.as_os_str().to_owned();
    public_key.push(".pub");
    let public_key = PathBuf::from(public_key);
    public_key.is_file().then_some(public_key)
}
