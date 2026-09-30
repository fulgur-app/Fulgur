use super::errors::{auth_request_error, is_authentication_rejection};
use super::methods::AllowedMethods;
use crate::fulgur::sync::ssh::auth::{ChallengeAnswer, CredentialPrompter};
use crate::fulgur::sync::ssh::error::SshError;
use ssh2::{KeyboardInteractivePrompt, Prompt, Session};
use zeroize::Zeroizing;

/// Answers keyboard-interactive prompts: the user's password goes to the first hidden
/// prompt, and every other prompt (e.g. a one-time code) is asked through the prompter.
struct KeyboardInteractiveResponder<'a> {
    password: Option<&'a Zeroizing<String>>,
    prompter: &'a mut dyn CredentialPrompter,
    cancelled: bool,
}

impl KeyboardInteractivePrompt for KeyboardInteractiveResponder<'_> {
    fn prompt(
        &mut self,
        _username: &str,
        instructions: &str,
        prompts: &[Prompt<'_>],
    ) -> Vec<String> {
        // ssh2 takes plain `String` responses, so these copies cannot be zeroized.
        prompts
            .iter()
            .map(|prompt| {
                if self.cancelled {
                    return String::new();
                }
                if !prompt.echo
                    && let Some(password) = self.password.take()
                {
                    return password.to_string();
                }
                match self
                    .prompter
                    .challenge(instructions, &prompt.text, prompt.echo)
                {
                    ChallengeAnswer::Response(response) => response.to_string(),
                    ChallengeAnswer::Cancel => {
                        self.cancelled = true;
                        String::new()
                    }
                }
            })
            .collect()
    }
}

/// Log in with a password, through the `password` method when the server accepts it and
/// through `keyboard-interactive` otherwise.
///
/// ### Arguments
/// - `session`: Session to authenticate.
/// - `user`: Remote username.
/// - `password`: Password typed by the user.
/// - `methods`: Methods the server accepts.
/// - `prompter`: Source of extra keyboard-interactive answers, such as one-time codes.
///
/// ### Errors
/// Returns `SshError::AuthCancelled` when the user cancels a keyboard-interactive
/// challenge and `SshError::ConnectionFailed` on any failure other than a rejection.
///
/// ### Returns
/// - `Ok(true)`: The password authenticated the session.
/// - `Ok(false)`: The server rejected the login.
/// - `Err(SshError)`: The user cancelled or the request failed for another reason.
pub(super) fn try_password(
    session: &Session,
    user: &str,
    password: &Zeroizing<String>,
    methods: AllowedMethods,
    prompter: &mut dyn CredentialPrompter,
) -> Result<bool, SshError> {
    if methods.password {
        return auth_request_outcome(session, session.userauth_password(user, password.as_str()));
    }
    let mut responder = KeyboardInteractiveResponder {
        password: Some(password),
        prompter,
        cancelled: false,
    };
    let result = session.userauth_keyboard_interactive(user, &mut responder);
    if responder.cancelled {
        return Err(SshError::AuthCancelled);
    }
    auth_request_outcome(session, result)
}

/// Interpret the result of a password or keyboard-interactive request.
///
/// ### Arguments
/// - `session`: Session the request was sent on.
/// - `result`: Raw ssh2 result of the request.
///
/// ### Errors
/// Returns `SshError::ConnectionFailed` on any failure other than a rejection.
///
/// ### Returns
/// - `Ok(true)`: The session is authenticated.
/// - `Ok(false)`: The server rejected the credentials.
/// - `Err(SshError)`: The request failed for another reason.
fn auth_request_outcome(
    session: &Session,
    result: Result<(), ssh2::Error>,
) -> Result<bool, SshError> {
    match result {
        Ok(()) => Ok(session.authenticated()),
        Err(error) if is_authentication_rejection(&error) => Ok(false),
        Err(error) => Err(auth_request_error(&error)),
    }
}

#[cfg(test)]
mod tests {
    use super::KeyboardInteractiveResponder;
    use crate::fulgur::sync::ssh::auth::{
        ChallengeAnswer, CredentialPrompter, LoginAnswer, PassphraseAnswer,
    };
    use ssh2::{KeyboardInteractivePrompt, Prompt};
    use std::borrow::Cow;
    use std::collections::VecDeque;
    use std::path::Path;
    use zeroize::Zeroizing;

    /// Prompter answering challenges from a script and recording the prompts it saw.
    struct ScriptedPrompter {
        answers: VecDeque<ChallengeAnswer>,
        seen: Vec<(String, bool)>,
    }

    impl ScriptedPrompter {
        fn new(answers: Vec<ChallengeAnswer>) -> Self {
            Self {
                answers: answers.into(),
                seen: Vec::new(),
            }
        }
    }

    impl CredentialPrompter for ScriptedPrompter {
        fn passphrase(&mut self, _key_path: &Path, _error: Option<String>) -> PassphraseAnswer {
            PassphraseAnswer::Cancel
        }

        fn login(&mut self, _password_allowed: bool, _error: Option<String>) -> LoginAnswer {
            LoginAnswer::Cancel
        }

        fn challenge(&mut self, _instructions: &str, prompt: &str, echo: bool) -> ChallengeAnswer {
            self.seen.push((prompt.to_string(), echo));
            self.answers.pop_front().unwrap_or(ChallengeAnswer::Cancel)
        }
    }

    fn prompt(text: &str, echo: bool) -> Prompt<'_> {
        Prompt {
            text: Cow::Borrowed(text),
            echo,
        }
    }

    fn response(text: &str) -> ChallengeAnswer {
        ChallengeAnswer::Response(Zeroizing::new(text.to_string()))
    }

    #[test]
    fn keyboard_interactive_sends_password_to_first_hidden_prompt_only() {
        let password = Zeroizing::new("hunter2".to_string());
        let mut prompter = ScriptedPrompter::new(vec![response("123456")]);
        let mut responder = KeyboardInteractiveResponder {
            password: Some(&password),
            prompter: &mut prompter,
            cancelled: false,
        };

        let first_round = responder.prompt("alice", "", &[prompt("Mot de passe :", false)]);
        let second_round = responder.prompt("alice", "", &[prompt("Verification code:", false)]);

        assert!(!responder.cancelled);
        assert_eq!(first_round, vec!["hunter2".to_string()]);
        assert_eq!(second_round, vec!["123456".to_string()]);
        assert_eq!(
            prompter.seen,
            vec![("Verification code:".to_string(), false)]
        );
    }

    #[test]
    fn keyboard_interactive_asks_visible_prompts_before_using_password() {
        let password = Zeroizing::new("hunter2".to_string());
        let mut prompter = ScriptedPrompter::new(vec![response("alice")]);
        let mut responder = KeyboardInteractiveResponder {
            password: Some(&password),
            prompter: &mut prompter,
            cancelled: false,
        };

        let responses = responder.prompt(
            "",
            "Welcome",
            &[prompt("Login:", true), prompt("Password:", false)],
        );

        assert_eq!(responses, vec!["alice".to_string(), "hunter2".to_string()]);
        assert_eq!(prompter.seen, vec![("Login:".to_string(), true)]);
    }

    #[test]
    fn keyboard_interactive_cancellation_stops_asking() {
        let password = Zeroizing::new("hunter2".to_string());
        let mut prompter = ScriptedPrompter::new(vec![ChallengeAnswer::Cancel]);
        let mut responder = KeyboardInteractiveResponder {
            password: Some(&password),
            prompter: &mut prompter,
            cancelled: false,
        };

        let responses = responder.prompt(
            "alice",
            "",
            &[prompt("Code:", true), prompt("Second code:", true)],
        );

        assert!(responder.cancelled);
        assert_eq!(responses, vec![String::new(), String::new()]);
        assert_eq!(prompter.seen.len(), 1);
    }
}
