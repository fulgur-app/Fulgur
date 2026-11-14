use super::super::remote_types::wait_for_user_answer;
use crate::fulgur::sync::ssh::auth::{
    ChallengeAnswer, CredentialPrompter, CredentialRequest, CredentialRequestKind, LoginAnswer,
    PassphraseAnswer,
};
use parking_lot::Mutex;
use std::{
    path::Path,
    sync::{Arc, atomic::AtomicBool, mpsc::Sender},
};

/// Credential prompter that hands prompts to the UI monitor task and blocks until answered.
pub(super) struct ChannelPrompter {
    pub(super) pending_request: Arc<Mutex<Option<CredentialRequest>>>,
    pub(super) host: String,
    pub(super) port: u16,
    pub(super) user: String,
    pub(super) timed_out: Arc<AtomicBool>,
}

impl ChannelPrompter {
    /// Post a prompt for the monitor task and wait for the user's answer.
    ///
    /// ### Arguments
    /// - `kind`: Builds the prompt around the channel the answer is sent on.
    ///
    /// ### Returns
    /// - `Some(T)`: The user's answer.
    /// - `None`: The dialog closed without answering, or the prompt timed out.
    fn ask<T>(&self, kind: impl FnOnce(Sender<T>) -> CredentialRequestKind) -> Option<T> {
        let (answer_tx, answer_rx) = std::sync::mpsc::channel();
        *self.pending_request.lock() = Some(CredentialRequest {
            host: self.host.clone(),
            port: self.port,
            user: self.user.clone(),
            kind: kind(answer_tx),
        });
        wait_for_user_answer(&answer_rx, &self.timed_out)
    }
}

impl CredentialPrompter for ChannelPrompter {
    fn passphrase(&mut self, key_path: &Path, error: Option<String>) -> PassphraseAnswer {
        self.ask(|answer_tx| CredentialRequestKind::Passphrase {
            key_path: key_path.to_path_buf(),
            error,
            answer_tx,
        })
        .unwrap_or(PassphraseAnswer::Cancel)
    }

    fn login(&mut self, password_allowed: bool, error: Option<String>) -> LoginAnswer {
        self.ask(|answer_tx| CredentialRequestKind::Login {
            password_allowed,
            error,
            answer_tx,
        })
        .unwrap_or(LoginAnswer::Cancel)
    }

    fn challenge(&mut self, instructions: &str, prompt: &str, echo: bool) -> ChallengeAnswer {
        self.ask(|answer_tx| CredentialRequestKind::Challenge {
            instructions: instructions.to_string(),
            prompt: prompt.to_string(),
            echo,
            answer_tx,
        })
        .unwrap_or(ChallengeAnswer::Cancel)
    }
}

#[cfg(test)]
mod tests {
    use super::ChannelPrompter;
    use crate::fulgur::sync::ssh::auth::{
        CredentialPrompter, CredentialRequest, CredentialRequestKind, LoginAnswer,
    };
    use parking_lot::Mutex;
    use std::{
        sync::{Arc, atomic::AtomicBool},
        thread,
        time::{Duration, Instant},
    };
    use zeroize::Zeroizing;

    /// Longest time a test waits for the worker thread to post its prompt.
    const POSTED_REQUEST_DEADLINE: Duration = Duration::from_secs(5);

    /// Build a prompter for `alice@example.com:2222` and the slot it posts requests to.
    ///
    /// ### Returns
    /// - `(ChannelPrompter, Arc<Mutex<Option<CredentialRequest>>>)`: The prompter and its
    ///   pending request slot, as read by the monitor task
    fn make_prompter() -> (ChannelPrompter, Arc<Mutex<Option<CredentialRequest>>>) {
        let pending_request = Arc::new(Mutex::new(None));
        let prompter = ChannelPrompter {
            pending_request: Arc::clone(&pending_request),
            host: "example.com".to_string(),
            port: 2222,
            user: "alice".to_string(),
            timed_out: Arc::new(AtomicBool::new(false)),
        };
        (prompter, pending_request)
    }

    /// Wait for the worker thread to post a prompt, then take it as the monitor task does.
    ///
    /// ### Arguments
    /// - `pending_request`: Slot the prompter posts its request to
    ///
    /// ### Returns
    /// - `CredentialRequest`: The posted request; panics after `POSTED_REQUEST_DEADLINE`
    fn take_posted_request(
        pending_request: &Mutex<Option<CredentialRequest>>,
    ) -> CredentialRequest {
        let deadline = Instant::now() + POSTED_REQUEST_DEADLINE;
        loop {
            if let Some(request) = pending_request.lock().take() {
                return request;
            }
            assert!(
                Instant::now() < deadline,
                "the prompter never posted a request"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }

    #[test]
    fn test_channel_prompter_login_forwards_answer_from_ui_thread() {
        let (mut prompter, pending_request) = make_prompter();
        let worker = thread::spawn(move || prompter.login(true, Some("denied".to_string())));

        let request = take_posted_request(&pending_request);
        assert_eq!(request.host, "example.com");
        assert_eq!(request.port, 2222);
        assert_eq!(request.user, "alice");
        let CredentialRequestKind::Login {
            password_allowed,
            error,
            answer_tx,
        } = request.kind
        else {
            panic!("expected a login request");
        };
        assert!(password_allowed);
        assert_eq!(error.as_deref(), Some("denied"));
        answer_tx
            .send(LoginAnswer::Password(Zeroizing::new("secret".to_string())))
            .expect("worker is waiting for the answer");

        let answer = worker.join().expect("worker thread panicked");
        assert!(matches!(answer, LoginAnswer::Password(password) if password.as_str() == "secret"));
    }

    #[test]
    fn test_channel_prompter_login_cancels_when_monitor_drops_request() {
        let (mut prompter, pending_request) = make_prompter();
        let worker = thread::spawn(move || prompter.login(true, None));

        // On timeout the monitor task drops the unanswered request, closing its channel.
        drop(take_posted_request(&pending_request));

        let answer = worker.join().expect("worker thread panicked");
        assert!(matches!(answer, LoginAnswer::Cancel));
    }
}
