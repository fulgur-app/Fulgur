mod errors;
mod methods;
mod password;
mod public_key;

use super::identities::{identity_files_for_host, public_key_blob};
use super::ssh_config::SshHostConfig;
use crate::fulgur::sync::ssh::auth::{CredentialPrompter, LoginAnswer, SshAuth};
use crate::fulgur::sync::ssh::error::SshError;
use errors::{auth_request_error, is_transport_error};
use methods::AllowedMethods;
use password::try_password;
use public_key::{KeyAttempt, KeyOutcome, authenticate_with_key_file, try_agent, try_key_file};
use ssh2::Session;

/// Number of times a passphrase or login prompt is repeated after a failed attempt.
const MAX_PROMPT_ATTEMPTS: usize = 3;

/// Authenticate a handshaken session, falling back from automatic methods to prompts.
///
/// Order, like OpenSSH: the previously successful credentials, then ssh-agent identities,
/// then key files from `~/.ssh/config` and the default key names (prompting for the
/// passphrase of encrypted keys the server accepts), then the login prompt.
///
/// ### Arguments
/// - `session`: Session that completed the handshake and host-key check.
/// - `alias`: Host as typed in the remote URL.
/// - `host_config`: Settings from `~/.ssh/config` for the host.
/// - `user`: Remote username.
/// - `preferred`: Credentials that authenticated this target before, tried first.
/// - `prompter`: Source of passphrases, passwords, and key files typed by the user.
///
/// ### Errors
/// Returns `SshError::AuthCancelled` when the user cancels a prompt,
/// `SshError::AuthFailed` when every method was rejected, and
/// `SshError::ConnectionFailed` on transport failures.
///
/// ### Returns
/// - `Ok(SshAuth)`: The credentials that authenticated the session.
/// - `Err(SshError)`: Authentication did not succeed.
pub(super) fn authenticate(
    session: &Session,
    alias: &str,
    host_config: &SshHostConfig,
    user: &str,
    preferred: Option<&SshAuth>,
    prompter: &mut dyn CredentialPrompter,
) -> Result<SshAuth, SshError> {
    let methods = match session.auth_methods(user) {
        Ok(list) => AllowedMethods::parse(list),
        Err(error) if is_transport_error(&error) => return Err(auth_request_error(&error)),
        Err(error) => {
            log::debug!("SSH server did not list its authentication methods: {error}");
            AllowedMethods::ALL
        }
    };
    if session.authenticated() {
        return Ok(SshAuth::Automatic);
    }

    if let Some(auth) = preferred
        && try_preferred(session, user, auth, methods, prompter)?
    {
        return Ok(auth.clone());
    }

    if methods.publickey {
        let identity_files = identity_files_for_host(host_config, alias, user);
        let agent_filter = host_config.identities_only.then(|| {
            identity_files
                .iter()
                .filter_map(|path| public_key_blob(path))
                .collect::<Vec<_>>()
        });
        if try_agent(session, user, agent_filter.as_deref())? {
            return Ok(SshAuth::Automatic);
        }
        for path in identity_files {
            if let KeyOutcome::Authenticated(auth) =
                authenticate_with_key_file(session, user, &path, prompter)?
            {
                return Ok(auth);
            }
        }
    }

    if methods.publickey || methods.accepts_password() {
        return authenticate_with_login_prompt(session, user, methods, prompter);
    }
    Err(SshError::AuthFailed)
}

/// Try credentials that authenticated this target before.
///
/// ### Arguments
/// - `session`: Session to authenticate.
/// - `user`: Remote username.
/// - `auth`: Cached credentials.
/// - `methods`: Methods the server accepts.
/// - `prompter`: Source of extra keyboard-interactive answers, such as one-time codes.
///
/// ### Errors
/// Returns `SshError::AuthCancelled` when the user cancels a keyboard-interactive
/// challenge and `SshError::ConnectionFailed` on transport failures.
///
/// ### Returns
/// - `Ok(true)`: The cached credentials authenticated the session.
/// - `Ok(false)`: They were rejected, or they are `Automatic` and left to discovery.
/// - `Err(SshError)`: The user cancelled or the connection failed.
fn try_preferred(
    session: &Session,
    user: &str,
    auth: &SshAuth,
    methods: AllowedMethods,
    prompter: &mut dyn CredentialPrompter,
) -> Result<bool, SshError> {
    match auth {
        SshAuth::Automatic => Ok(false),
        SshAuth::KeyFile { path, passphrase } => Ok(methods.publickey
            && matches!(
                try_key_file(
                    session,
                    user,
                    path,
                    passphrase.as_deref().map(String::as_str)
                )?,
                KeyAttempt::Accepted
            )),
        SshAuth::Password(password) => {
            if methods.accepts_password() {
                try_password(session, user, password, methods, prompter)
            } else {
                Ok(false)
            }
        }
    }
}

/// Prompt for a password or a key file until one authenticates the session.
///
/// ### Arguments
/// - `session`: Session to authenticate.
/// - `user`: Remote username.
/// - `methods`: Methods the server accepts.
/// - `prompter`: Source of the password, key file, and keyboard-interactive answers.
///
/// ### Errors
/// Returns `SshError::AuthCancelled` when the user cancels, `SshError::AuthFailed` after
/// `MAX_PROMPT_ATTEMPTS` rejected answers, and `SshError::ConnectionFailed` on transport
/// failures.
///
/// ### Returns
/// - `Ok(SshAuth)`: The credentials that authenticated the session.
/// - `Err(SshError)`: Authentication did not succeed.
fn authenticate_with_login_prompt(
    session: &Session,
    user: &str,
    methods: AllowedMethods,
    prompter: &mut dyn CredentialPrompter,
) -> Result<SshAuth, SshError> {
    let mut error = None;
    for _ in 0..MAX_PROMPT_ATTEMPTS {
        match prompter.login(methods.accepts_password(), error.take()) {
            LoginAnswer::Password(password) => {
                if try_password(session, user, &password, methods, prompter)? {
                    return Ok(SshAuth::Password(password));
                }
                let rejection = if methods.password {
                    "The server rejected this password."
                } else {
                    "The server rejected the login."
                };
                error = Some(rejection.to_string());
            }
            LoginAnswer::KeyFile(path) if !path.is_file() => {
                error = Some(format!("Key file not found: {}", path.display()));
            }
            LoginAnswer::KeyFile(path) => {
                match authenticate_with_key_file(session, user, &path, prompter)? {
                    KeyOutcome::Authenticated(auth) => return Ok(auth),
                    KeyOutcome::Rejected => {
                        error = Some(format!("The server rejected the key {}.", path.display()));
                    }
                    KeyOutcome::Unusable => {
                        error = Some(format!("Cannot use the key {}.", path.display()));
                    }
                    KeyOutcome::Skipped => {}
                }
            }
            LoginAnswer::Cancel => return Err(SshError::AuthCancelled),
        }
    }
    Err(SshError::AuthFailed)
}
