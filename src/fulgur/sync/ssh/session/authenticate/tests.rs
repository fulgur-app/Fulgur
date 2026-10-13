use super::fake_session::{Attempt, FakeIdentity, FakeServer, ScriptedPrompter, ServerKey};
use super::{MAX_PROMPT_ATTEMPTS, authenticate};
use crate::fulgur::sync::ssh::auth::{LoginAnswer, PassphraseAnswer, SshAuth};
use crate::fulgur::sync::ssh::error::SshError;
use crate::fulgur::sync::ssh::session::identities::public_key_blob;
use crate::fulgur::sync::ssh::session::ssh_config::SshHostConfig;
use std::path::{Path, PathBuf};
use tempfile::TempDir;
use zeroize::Zeroizing;

const WORK_PUBLIC_KEY: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIPATAQcIp4UXhvzzbDWUswYaIikg22nEzznF2gsIKHpn work\n";
const ENCRYPTED_KEY: &str =
    "-----BEGIN ENCRYPTED PRIVATE KEY-----\nAAAA\n-----END ENCRYPTED PRIVATE KEY-----\n";

/// Create a home directory whose `.ssh` holds the given private key files.
fn ssh_home(keys: &[&str]) -> TempDir {
    let home = tempfile::tempdir().expect("failed to create temp dir");
    let ssh_dir = home.path().join(".ssh");
    std::fs::create_dir(&ssh_dir).expect("failed to create .ssh");
    for key in keys {
        std::fs::write(ssh_dir.join(key), "fixture key").expect("failed to write key");
    }
    home
}

fn key_path(home: &TempDir, name: &str) -> PathBuf {
    home.path().join(".ssh").join(name)
}

fn password(text: &str) -> Zeroizing<String> {
    Zeroizing::new(text.to_string())
}

fn run(
    server: FakeServer,
    host_config: &SshHostConfig,
    home: Option<&Path>,
    preferred: Option<&SshAuth>,
    prompter: &mut ScriptedPrompter,
) -> (Result<SshAuth, SshError>, Vec<Attempt>) {
    let session = server.into_session();
    let result = authenticate(
        &session,
        "server",
        host_config,
        "alice",
        home,
        preferred,
        prompter,
    );
    (result, session.attempts())
}

#[test]
fn authenticated_session_skips_every_method() {
    let server = FakeServer {
        methods: "publickey,password",
        authenticated_up_front: true,
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter::default();

    let (result, attempts) = run(server, &SshHostConfig::default(), None, None, &mut prompter);

    assert!(matches!(result, Ok(SshAuth::Automatic)));
    assert_eq!(attempts, Vec::<Attempt>::new());
    assert_eq!(prompter.login_errors, Vec::<Option<String>>::new());
}

#[test]
fn preferred_credentials_short_circuit_discovery() {
    let server = FakeServer {
        methods: "publickey,password",
        agent: Some(vec![FakeIdentity {
            blob: vec![1],
            comment: "agent",
        }]),
        accepted_agent_identities: vec!["agent"],
        password: Some("cached"),
        ..FakeServer::default()
    };
    let preferred = SshAuth::Password(password("cached"));
    let mut prompter = ScriptedPrompter::default();

    let (result, attempts) = run(
        server,
        &SshHostConfig::default(),
        None,
        Some(&preferred),
        &mut prompter,
    );

    assert!(matches!(result, Ok(SshAuth::Password(ref p)) if p.as_str() == "cached"));
    assert_eq!(attempts, vec![Attempt::Password("cached".to_string())]);
}

#[test]
fn rejected_preferred_credentials_fall_back_to_agent_identities_in_order() {
    let server = FakeServer {
        methods: "publickey,password",
        agent: Some(vec![
            FakeIdentity {
                blob: vec![1],
                comment: "first",
            },
            FakeIdentity {
                blob: vec![2],
                comment: "second",
            },
        ]),
        accepted_agent_identities: vec!["second"],
        ..FakeServer::default()
    };
    let preferred = SshAuth::Password(password("stale"));
    let mut prompter = ScriptedPrompter::default();

    let (result, attempts) = run(
        server,
        &SshHostConfig::default(),
        None,
        Some(&preferred),
        &mut prompter,
    );

    assert!(matches!(result, Ok(SshAuth::Automatic)));
    assert_eq!(
        attempts,
        vec![
            Attempt::Password("stale".to_string()),
            Attempt::Agent("first".to_string()),
            Attempt::Agent("second".to_string()),
        ]
    );
}

#[test]
fn identities_only_offers_only_configured_agent_identities() {
    let home = ssh_home(&["id_work"]);
    let work_key = key_path(&home, "id_work");
    std::fs::write(home.path().join(".ssh/id_work.pub"), WORK_PUBLIC_KEY)
        .expect("failed to write public key");
    let work_blob = public_key_blob(&work_key).expect("public key is readable");
    let host_config = SshHostConfig::parse(
        "Host server\n  IdentitiesOnly yes\n  IdentityFile ~/.ssh/id_work\n",
        "server",
    );
    let server = FakeServer {
        methods: "publickey",
        agent: Some(vec![
            FakeIdentity {
                blob: vec![9, 9, 9],
                comment: "unrelated",
            },
            FakeIdentity {
                blob: work_blob,
                comment: "work",
            },
        ]),
        accepted_agent_identities: vec!["unrelated"],
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter::default();

    let (result, attempts) = run(server, &host_config, Some(home.path()), None, &mut prompter);

    assert!(matches!(result, Err(SshError::AuthCancelled)));
    assert_eq!(
        attempts,
        vec![
            Attempt::Agent("work".to_string()),
            Attempt::KeyFile(work_key),
        ]
    );
}

#[test]
fn configured_identity_file_is_used_before_login_prompt() {
    let home = ssh_home(&["id_work", "id_ed25519"]);
    let work_key = key_path(&home, "id_work");
    let host_config =
        SshHostConfig::parse("Host server\n  IdentityFile ~/.ssh/id_work\n", "server");
    let server = FakeServer {
        methods: "publickey,password",
        agent: Some(Vec::new()),
        accepted_keys: vec![ServerKey {
            path: work_key.clone(),
            passphrase: None,
        }],
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter::default();

    let (result, attempts) = run(server, &host_config, Some(home.path()), None, &mut prompter);

    assert!(matches!(
        result,
        Ok(SshAuth::KeyFile { ref path, passphrase: None }) if *path == work_key
    ));
    assert_eq!(attempts, vec![Attempt::KeyFile(work_key)]);
    assert_eq!(prompter.login_errors, Vec::<Option<String>>::new());
}

#[test]
fn encrypted_key_asks_passphrase_until_it_unlocks() {
    let home = ssh_home(&[]);
    let key = key_path(&home, "id_ed25519");
    std::fs::write(&key, ENCRYPTED_KEY).expect("failed to write key");
    let server = FakeServer {
        methods: "publickey",
        accepted_keys: vec![ServerKey {
            path: key.clone(),
            passphrase: Some("secret"),
        }],
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter {
        passphrases: [
            PassphraseAnswer::Passphrase(password("wrong")),
            PassphraseAnswer::Passphrase(password("secret")),
        ]
        .into(),
        ..ScriptedPrompter::default()
    };

    let (result, attempts) = run(
        server,
        &SshHostConfig::default(),
        Some(home.path()),
        None,
        &mut prompter,
    );

    assert!(matches!(
        result,
        Ok(SshAuth::KeyFile { ref path, passphrase: Some(ref p) })
            if *path == key && p.as_str() == "secret"
    ));
    assert_eq!(attempts.len(), 3);
    assert_eq!(
        prompter.passphrase_errors,
        vec![None, Some("Wrong passphrase, try again.".to_string())]
    );
}

#[test]
fn login_prompt_gives_up_after_max_attempts() {
    let server = FakeServer {
        methods: "password",
        password: Some("right"),
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter {
        logins: (0..MAX_PROMPT_ATTEMPTS)
            .map(|attempt| LoginAnswer::Password(password(&format!("wrong-{attempt}"))))
            .chain([LoginAnswer::Password(password("right"))])
            .collect(),
        ..ScriptedPrompter::default()
    };

    let (result, attempts) = run(server, &SshHostConfig::default(), None, None, &mut prompter);

    assert!(matches!(result, Err(SshError::AuthFailed)));
    assert_eq!(attempts.len(), MAX_PROMPT_ATTEMPTS);
    assert_eq!(prompter.logins.len(), 1, "the fourth answer is never asked");
    let rejection = Some("The server rejected this password.".to_string());
    assert_eq!(
        prompter.login_errors,
        vec![None, rejection.clone(), rejection]
    );
}

#[test]
fn login_prompt_cancel_aborts_authentication() {
    let server = FakeServer {
        methods: "publickey,password",
        password: Some("right"),
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter {
        logins: [LoginAnswer::Cancel].into(),
        ..ScriptedPrompter::default()
    };

    let (result, attempts) = run(server, &SshHostConfig::default(), None, None, &mut prompter);

    assert!(matches!(result, Err(SshError::AuthCancelled)));
    assert_eq!(attempts, Vec::<Attempt>::new());
}

#[test]
fn missing_key_file_is_reported_without_a_server_request() {
    let missing = PathBuf::from("/nonexistent/fulgur/id_missing");
    let server = FakeServer {
        methods: "publickey,password",
        password: Some("right"),
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter {
        logins: [
            LoginAnswer::KeyFile(missing.clone()),
            LoginAnswer::Password(password("right")),
        ]
        .into(),
        ..ScriptedPrompter::default()
    };

    let (result, attempts) = run(server, &SshHostConfig::default(), None, None, &mut prompter);

    assert!(matches!(result, Ok(SshAuth::Password(ref p)) if p.as_str() == "right"));
    assert_eq!(attempts, vec![Attempt::Password("right".to_string())]);
    assert_eq!(
        prompter.login_errors,
        vec![
            None,
            Some(format!("Key file not found: {}", missing.display()))
        ]
    );
}

#[test]
fn keyboard_interactive_carries_the_password_when_password_method_is_absent() {
    let server = FakeServer {
        methods: "keyboard-interactive",
        password: Some("right"),
        ..FakeServer::default()
    };
    let mut prompter = ScriptedPrompter {
        logins: [
            LoginAnswer::Password(password("wrong")),
            LoginAnswer::Password(password("right")),
        ]
        .into(),
        ..ScriptedPrompter::default()
    };

    let (result, attempts) = run(server, &SshHostConfig::default(), None, None, &mut prompter);

    assert!(matches!(result, Ok(SshAuth::Password(ref p)) if p.as_str() == "right"));
    assert_eq!(
        attempts,
        vec![Attempt::KeyboardInteractive, Attempt::KeyboardInteractive]
    );
    assert_eq!(
        prompter.login_errors,
        vec![None, Some("The server rejected the login.".to_string())]
    );
}
