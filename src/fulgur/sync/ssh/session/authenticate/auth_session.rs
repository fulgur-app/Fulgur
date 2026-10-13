use ssh2::{Agent, KeyboardInteractivePrompt, PublicKey, Session};
use std::path::Path;

/// An identity held by ssh-agent.
pub(in crate::fulgur::sync::ssh::session) trait AgentIdentity {
    /// Read the SSH wire-format public key blob of the identity.
    ///
    /// ### Returns
    /// - `&[u8]`: The public key blob, comparable with `public_key_blob`.
    fn blob(&self) -> &[u8];

    /// Read the comment attached to the identity, usually its file name or e-mail address.
    ///
    /// ### Returns
    /// - `&str`: The identity comment.
    fn comment(&self) -> &str;
}

/// A connection to ssh-agent whose identities have been listed.
pub(in crate::fulgur::sync::ssh::session) trait AgentConnection {
    type Identity: AgentIdentity;

    /// List the identities held by the agent.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the identity list cannot be read.
    ///
    /// ### Returns
    /// - `Ok(Vec<Identity>)`: The identities, in agent order.
    /// - `Err(ssh2::Error)`: The list could not be read.
    fn identities(&self) -> Result<Vec<Self::Identity>, ssh2::Error>;

    /// Send one public-key authentication request signed by the agent.
    ///
    /// ### Arguments
    /// - `user`: Remote username.
    /// - `identity`: Agent identity to offer.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the server rejects the identity or the request fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The request succeeded.
    /// - `Err(ssh2::Error)`: The identity was rejected or the request failed.
    fn userauth(&self, user: &str, identity: &Self::Identity) -> Result<(), ssh2::Error>;

    /// Close the connection to the agent.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the agent connection cannot be closed cleanly.
    ///
    /// ### Returns
    /// - `Ok(())`: The connection was closed.
    /// - `Err(ssh2::Error)`: Closing failed.
    fn disconnect(&mut self) -> Result<(), ssh2::Error>;
}

/// The SSH session calls used by `authenticate`, so the authentication order can be
/// driven by a scripted session in tests.
pub(in crate::fulgur::sync::ssh::session) trait AuthSession {
    type Agent: AgentConnection;

    /// Ask the server which authentication methods it accepts for a user.
    ///
    /// ### Arguments
    /// - `user`: Remote username.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the server does not answer, or authenticated the user
    /// with the `none` method.
    ///
    /// ### Returns
    /// - `Ok(&str)`: Comma-separated method names, e.g. `"publickey,password"`.
    /// - `Err(ssh2::Error)`: The list could not be obtained.
    fn auth_methods(&self, user: &str) -> Result<&str, ssh2::Error>;

    /// Report whether the session is authenticated.
    ///
    /// ### Returns
    /// - `true`: The session is authenticated.
    /// - `false`: Authentication has not succeeded yet.
    fn authenticated(&self) -> bool;

    /// Connect to the running ssh-agent and list its identities.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when no agent is reachable or it cannot list identities.
    ///
    /// ### Returns
    /// - `Ok(Agent)`: The connected agent.
    /// - `Err(ssh2::Error)`: The agent is unavailable.
    fn connect_agent(&self) -> Result<Self::Agent, ssh2::Error>;

    /// Send one public-key authentication request for a private key file.
    ///
    /// ### Arguments
    /// - `user`: Remote username.
    /// - `public_key`: Public key file, or `None` to derive it from the private key.
    /// - `private_key`: Private key file.
    /// - `passphrase`: Passphrase for an encrypted key.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the server rejects the key, the key cannot be loaded,
    /// or the request fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The request succeeded.
    /// - `Err(ssh2::Error)`: The key was rejected or unusable, or the request failed.
    fn userauth_pubkey_file(
        &self,
        user: &str,
        public_key: Option<&Path>,
        private_key: &Path,
        passphrase: Option<&str>,
    ) -> Result<(), ssh2::Error>;

    /// Send a `password` authentication request.
    ///
    /// ### Arguments
    /// - `user`: Remote username.
    /// - `password`: Password typed by the user.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the server rejects the password or the request fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The request succeeded.
    /// - `Err(ssh2::Error)`: The password was rejected or the request failed.
    fn userauth_password(&self, user: &str, password: &str) -> Result<(), ssh2::Error>;

    /// Run a `keyboard-interactive` authentication exchange.
    ///
    /// ### Arguments
    /// - `user`: Remote username.
    /// - `prompter`: Answers the questions sent by the server.
    ///
    /// ### Errors
    /// Returns an `ssh2::Error` when the server rejects the answers or the request fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The exchange succeeded.
    /// - `Err(ssh2::Error)`: The answers were rejected or the request failed.
    fn userauth_keyboard_interactive<P: KeyboardInteractivePrompt>(
        &self,
        user: &str,
        prompter: &mut P,
    ) -> Result<(), ssh2::Error>;
}

impl AgentIdentity for PublicKey {
    fn blob(&self) -> &[u8] {
        PublicKey::blob(self)
    }

    fn comment(&self) -> &str {
        PublicKey::comment(self)
    }
}

impl AgentConnection for Agent {
    type Identity = PublicKey;

    fn identities(&self) -> Result<Vec<PublicKey>, ssh2::Error> {
        Agent::identities(self)
    }

    fn userauth(&self, user: &str, identity: &PublicKey) -> Result<(), ssh2::Error> {
        Agent::userauth(self, user, identity)
    }

    fn disconnect(&mut self) -> Result<(), ssh2::Error> {
        Agent::disconnect(self)
    }
}

impl AuthSession for Session {
    type Agent = Agent;

    fn auth_methods(&self, user: &str) -> Result<&str, ssh2::Error> {
        Session::auth_methods(self, user)
    }

    fn authenticated(&self) -> bool {
        Session::authenticated(self)
    }

    fn connect_agent(&self) -> Result<Agent, ssh2::Error> {
        let mut agent = self.agent()?;
        agent.connect()?;
        agent.list_identities()?;
        Ok(agent)
    }

    fn userauth_pubkey_file(
        &self,
        user: &str,
        public_key: Option<&Path>,
        private_key: &Path,
        passphrase: Option<&str>,
    ) -> Result<(), ssh2::Error> {
        Session::userauth_pubkey_file(self, user, public_key, private_key, passphrase)
    }

    fn userauth_password(&self, user: &str, password: &str) -> Result<(), ssh2::Error> {
        Session::userauth_password(self, user, password)
    }

    fn userauth_keyboard_interactive<P: KeyboardInteractivePrompt>(
        &self,
        user: &str,
        prompter: &mut P,
    ) -> Result<(), ssh2::Error> {
        Session::userauth_keyboard_interactive(self, user, prompter)
    }
}
