use super::auth_session::{AgentConnection, AgentIdentity, AuthSession};
use crate::fulgur::sync::ssh::auth::{
    ChallengeAnswer, CredentialPrompter, LoginAnswer, PassphraseAnswer,
};
use ssh2::{ErrorCode, KeyboardInteractivePrompt, Prompt};
use std::borrow::Cow;
use std::cell::{Cell, RefCell};
use std::collections::VecDeque;
use std::path::{Path, PathBuf};
use std::rc::Rc;

const LIBSSH2_ERROR_FILE: i32 = -16;
const LIBSSH2_ERROR_AUTHENTICATION_FAILED: i32 = -18;
const LIBSSH2_ERROR_AGENT_PROTOCOL: i32 = -42;

/// An authentication request received by the fake server, in arrival order.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum Attempt {
    Agent(String),
    KeyFile(PathBuf),
    Password(String),
    KeyboardInteractive,
}

/// A private key file known to the fake server.
pub(super) struct ServerKey {
    pub(super) path: PathBuf,
    /// Passphrase the key file is encrypted with, if any.
    pub(super) passphrase: Option<&'static str>,
}

/// An ssh-agent identity held by the fake agent.
#[derive(Clone)]
pub(super) struct FakeIdentity {
    pub(super) blob: Vec<u8>,
    pub(super) comment: &'static str,
}

impl AgentIdentity for FakeIdentity {
    fn blob(&self) -> &[u8] {
        &self.blob
    }

    fn comment(&self) -> &str {
        self.comment
    }
}

/// Scripted server behavior.
#[derive(Default)]
pub(super) struct FakeServer {
    /// Value returned by `auth_methods`.
    pub(super) methods: &'static str,
    /// The session is authenticated before any request (`none` method).
    pub(super) authenticated_up_front: bool,
    /// Identities held by the agent; `None` when no agent is running.
    pub(super) agent: Option<Vec<FakeIdentity>>,
    /// Agent identity comments the server accepts.
    pub(super) accepted_agent_identities: Vec<&'static str>,
    /// Key files the server accepts.
    pub(super) accepted_keys: Vec<ServerKey>,
    /// Password the server accepts.
    pub(super) password: Option<&'static str>,
}

impl FakeServer {
    /// Wrap the scripted server in a session.
    ///
    /// ### Returns
    /// - `FakeSession`: Session backed by this server.
    pub(super) fn into_session(self) -> FakeSession {
        FakeSession(Rc::new(Connection {
            authenticated: Cell::new(self.authenticated_up_front),
            attempts: RefCell::new(Vec::new()),
            server: self,
        }))
    }
}

/// A connection to a `FakeServer`, shared by the session and its agent.
struct Connection {
    server: FakeServer,
    authenticated: Cell<bool>,
    attempts: RefCell<Vec<Attempt>>,
}

impl Connection {
    /// Record a request, then accept or reject it.
    ///
    /// ### Arguments
    /// - `attempt`: The request received.
    /// - `accepted`: Whether the server accepts it.
    /// - `rejection_code`: libssh2 error code returned when it is not accepted.
    ///
    /// ### Errors
    /// Returns the `ssh2::Error` built from `rejection_code` when the request is refused.
    ///
    /// ### Returns
    /// - `Ok(())`: The session is now authenticated.
    /// - `Err(ssh2::Error)`: The request was refused.
    fn answer(
        &self,
        attempt: Attempt,
        accepted: bool,
        rejection_code: i32,
    ) -> Result<(), ssh2::Error> {
        self.attempts.borrow_mut().push(attempt);
        if !accepted {
            return Err(ssh2::Error::from_errno(ErrorCode::Session(rejection_code)));
        }
        self.authenticated.set(true);
        Ok(())
    }
}

/// Session talking to a `FakeServer`.
pub(super) struct FakeSession(Rc<Connection>);

impl FakeSession {
    /// List the requests the server received so far.
    ///
    /// ### Returns
    /// - `Vec<Attempt>`: Requests in arrival order.
    pub(super) fn attempts(&self) -> Vec<Attempt> {
        self.0.attempts.borrow().clone()
    }
}

/// Agent connection of a `FakeSession`.
pub(super) struct FakeAgent(Rc<Connection>);

impl AgentConnection for FakeAgent {
    type Identity = FakeIdentity;

    fn identities(&self) -> Result<Vec<FakeIdentity>, ssh2::Error> {
        Ok(self.0.server.agent.clone().unwrap_or_default())
    }

    fn userauth(&self, _user: &str, identity: &FakeIdentity) -> Result<(), ssh2::Error> {
        let accepted = self
            .0
            .server
            .accepted_agent_identities
            .contains(&identity.comment);
        self.0.answer(
            Attempt::Agent(identity.comment.to_string()),
            accepted,
            LIBSSH2_ERROR_AUTHENTICATION_FAILED,
        )
    }

    fn disconnect(&mut self) -> Result<(), ssh2::Error> {
        Ok(())
    }
}

impl AuthSession for FakeSession {
    type Agent = FakeAgent;

    fn auth_methods(&self, _user: &str) -> Result<&str, ssh2::Error> {
        Ok(self.0.server.methods)
    }

    fn authenticated(&self) -> bool {
        self.0.authenticated.get()
    }

    fn connect_agent(&self) -> Result<FakeAgent, ssh2::Error> {
        match self.0.server.agent {
            Some(_) => Ok(FakeAgent(Rc::clone(&self.0))),
            None => Err(ssh2::Error::from_errno(ErrorCode::Session(
                LIBSSH2_ERROR_AGENT_PROTOCOL,
            ))),
        }
    }

    fn userauth_pubkey_file(
        &self,
        _user: &str,
        _public_key: Option<&Path>,
        private_key: &Path,
        passphrase: Option<&str>,
    ) -> Result<(), ssh2::Error> {
        let key = self
            .0
            .server
            .accepted_keys
            .iter()
            .find(|key| key.path == private_key);
        let wrong_passphrase = key.is_some_and(|key| {
            key.passphrase
                .is_some_and(|expected| passphrase != Some(expected))
        });
        let rejection_code = if wrong_passphrase {
            LIBSSH2_ERROR_FILE
        } else {
            LIBSSH2_ERROR_AUTHENTICATION_FAILED
        };
        self.0.answer(
            Attempt::KeyFile(private_key.to_path_buf()),
            key.is_some() && !wrong_passphrase,
            rejection_code,
        )
    }

    fn userauth_password(&self, _user: &str, password: &str) -> Result<(), ssh2::Error> {
        let accepted = self.0.server.password == Some(password);
        self.0.answer(
            Attempt::Password(password.to_string()),
            accepted,
            LIBSSH2_ERROR_AUTHENTICATION_FAILED,
        )
    }

    fn userauth_keyboard_interactive<P: KeyboardInteractivePrompt>(
        &self,
        user: &str,
        prompter: &mut P,
    ) -> Result<(), ssh2::Error> {
        let password_prompt = Prompt {
            text: Cow::Borrowed("Password:"),
            echo: false,
        };
        let responses = prompter.prompt(user, "", &[password_prompt]);
        let accepted = responses.first().map(String::as_str) == self.0.server.password;
        self.0.answer(
            Attempt::KeyboardInteractive,
            accepted,
            LIBSSH2_ERROR_AUTHENTICATION_FAILED,
        )
    }
}

/// Prompter answering from scripts and recording the error messages it was shown.
#[derive(Default)]
pub(super) struct ScriptedPrompter {
    pub(super) logins: VecDeque<LoginAnswer>,
    pub(super) passphrases: VecDeque<PassphraseAnswer>,
    pub(super) login_errors: Vec<Option<String>>,
    pub(super) passphrase_errors: Vec<Option<String>>,
}

impl CredentialPrompter for ScriptedPrompter {
    fn passphrase(&mut self, _key_path: &Path, error: Option<String>) -> PassphraseAnswer {
        self.passphrase_errors.push(error);
        self.passphrases
            .pop_front()
            .unwrap_or(PassphraseAnswer::Cancel)
    }

    fn login(&mut self, _password_allowed: bool, error: Option<String>) -> LoginAnswer {
        self.login_errors.push(error);
        self.logins.pop_front().unwrap_or(LoginAnswer::Cancel)
    }

    fn challenge(&mut self, _instructions: &str, _prompt: &str, _echo: bool) -> ChallengeAnswer {
        ChallengeAnswer::Cancel
    }
}
