use sha2::{Digest, Sha256};
use std::path::{Path, PathBuf};
use std::sync::mpsc::Sender;
use zeroize::Zeroizing;

/// Credentials that authenticated an SSH session, cached in memory for reconnects.
///
/// `Debug` is intentionally not derived so secrets never end up in logs.
#[derive(Clone)]
pub enum SshAuth {
    /// Authenticated without user input: an ssh-agent identity, or the server required
    /// no credentials. Reconnects run the automatic discovery again.
    Automatic,
    /// A private key file, with the passphrase that unlocked it when the key is encrypted.
    KeyFile {
        path: PathBuf,
        passphrase: Option<Zeroizing<String>>,
    },
    /// Password authentication.
    Password(Zeroizing<String>),
}

impl SshAuth {
    /// Compute a SHA-256 digest identifying these credentials without exposing secrets.
    ///
    /// ### Returns
    /// - `[u8; 32]`: Digest that differs whenever the method, key path, or secret differs.
    #[must_use]
    pub fn digest(&self) -> [u8; 32] {
        let mut hasher = Sha256::new();
        match self {
            SshAuth::Automatic => hasher.update(b"automatic"),
            SshAuth::KeyFile { path, passphrase } => {
                hasher.update(b"key-file\0");
                hasher.update(path.as_os_str().as_encoded_bytes());
                hasher.update(b"\0");
                if let Some(passphrase) = passphrase {
                    hasher.update(passphrase.as_bytes());
                }
            }
            SshAuth::Password(password) => {
                hasher.update(b"password\0");
                hasher.update(password.as_bytes());
            }
        }
        let mut digest = [0u8; 32];
        digest.copy_from_slice(&hasher.finalize());
        digest
    }
}

/// Answer to a key passphrase prompt.
pub enum PassphraseAnswer {
    /// Passphrase entered by the user.
    Passphrase(Zeroizing<String>),
    /// Do not use this key and move on to the next authentication method.
    Skip,
    /// Abort the connection.
    Cancel,
}

/// Answer to the login prompt shown once automatic authentication has failed.
pub enum LoginAnswer {
    /// Password entered by the user.
    Password(Zeroizing<String>),
    /// Private key file chosen by the user.
    KeyFile(PathBuf),
    /// Abort the connection.
    Cancel,
}

/// Answer to a keyboard-interactive challenge other than the password, e.g. a one-time code.
pub enum ChallengeAnswer {
    /// Response typed by the user.
    Response(Zeroizing<String>),
    /// Abort the connection.
    Cancel,
}

/// Source of interactive credentials used while authenticating an SSH session.
pub trait CredentialPrompter {
    /// Ask for the passphrase of an encrypted private key the server accepts.
    ///
    /// ### Arguments
    /// - `key_path`: Private key file to unlock.
    /// - `error`: Message explaining why a previous attempt failed, if any.
    ///
    /// ### Returns
    /// - `PassphraseAnswer`: The passphrase, or the decision to skip the key or cancel.
    fn passphrase(&mut self, key_path: &Path, error: Option<String>) -> PassphraseAnswer;

    /// Ask for a password or a private key file.
    ///
    /// ### Arguments
    /// - `password_allowed`: Whether the server accepts password authentication.
    /// - `error`: Message explaining why a previous attempt failed, if any.
    ///
    /// ### Returns
    /// - `LoginAnswer`: The password, the key file, or the decision to cancel.
    fn login(&mut self, password_allowed: bool, error: Option<String>) -> LoginAnswer;

    /// Ask a keyboard-interactive question sent by the server, e.g. a one-time code.
    ///
    /// ### Arguments
    /// - `instructions`: Informational text sent by the server; may be empty.
    /// - `prompt`: The question, as worded by the server.
    /// - `echo`: Whether the answer may be displayed while typed.
    ///
    /// ### Returns
    /// - `ChallengeAnswer`: The response, or the decision to cancel.
    fn challenge(&mut self, instructions: &str, prompt: &str, echo: bool) -> ChallengeAnswer;
}

/// Prompter for contexts without UI: every prompt is declined.
pub struct NoPrompt;

impl CredentialPrompter for NoPrompt {
    fn passphrase(&mut self, _key_path: &Path, _error: Option<String>) -> PassphraseAnswer {
        PassphraseAnswer::Cancel
    }

    fn login(&mut self, _password_allowed: bool, _error: Option<String>) -> LoginAnswer {
        LoginAnswer::Cancel
    }

    fn challenge(&mut self, _instructions: &str, _prompt: &str, _echo: bool) -> ChallengeAnswer {
        ChallengeAnswer::Cancel
    }
}

/// A credential prompt posted by an SSH worker thread for the UI to display.
pub struct CredentialRequest {
    /// Hostname of the remote server.
    pub host: String,
    /// SSH port of the remote server.
    pub port: u16,
    /// Username being authenticated.
    pub user: String,
    /// What to ask, with the channel that unblocks the worker once answered.
    pub kind: CredentialRequestKind,
}

/// The prompt to display for a `CredentialRequest`.
pub enum CredentialRequestKind {
    /// Unlock an encrypted private key.
    Passphrase {
        key_path: PathBuf,
        error: Option<String>,
        answer_tx: Sender<PassphraseAnswer>,
    },
    /// Log in with a password or a private key file.
    Login {
        password_allowed: bool,
        error: Option<String>,
        answer_tx: Sender<LoginAnswer>,
    },
    /// Answer a keyboard-interactive question sent by the server.
    Challenge {
        instructions: String,
        prompt: String,
        echo: bool,
        answer_tx: Sender<ChallengeAnswer>,
    },
}

#[cfg(test)]
mod tests {
    use super::SshAuth;
    use std::path::PathBuf;
    use zeroize::Zeroizing;

    #[test]
    fn digest_differs_between_methods_and_secrets() {
        let password_a = SshAuth::Password(Zeroizing::new("hunter2".to_string()));
        let password_b = SshAuth::Password(Zeroizing::new("trustno1".to_string()));
        let key = SshAuth::KeyFile {
            path: PathBuf::from("/home/alice/.ssh/id_ed25519"),
            passphrase: Some(Zeroizing::new("hunter2".to_string())),
        };
        let digests = [
            SshAuth::Automatic.digest(),
            password_a.digest(),
            password_b.digest(),
            key.digest(),
        ];
        for (index, digest) in digests.iter().enumerate() {
            for other in &digests[index + 1..] {
                assert_ne!(digest, other);
            }
        }
    }

    #[test]
    fn digest_is_stable_for_same_credentials() {
        let key = || SshAuth::KeyFile {
            path: PathBuf::from("/home/alice/.ssh/id_ed25519"),
            passphrase: None,
        };
        assert_eq!(key().digest(), key().digest());
    }
}
