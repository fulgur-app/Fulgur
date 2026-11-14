use ssh_key::{PublicKey, known_hosts::KnownHosts};

pub(super) const SERVER_KEY_OPENSSH: &str =
    "ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIJdD7y3aLq454yWBdwLWbieU1ebz9/cu7/QEXn9OIeZJ server";

pub(super) fn parse_known_host_entries(input: &str) -> Vec<ssh_key::known_hosts::Entry> {
    KnownHosts::new(input)
        .collect::<ssh_key::Result<Vec<_>>>()
        .expect("failed to parse known_hosts entries")
}

pub(super) fn server_key() -> PublicKey {
    PublicKey::from_openssh(SERVER_KEY_OPENSSH).expect("failed to parse server key")
}

pub(super) fn server_key_bytes() -> Vec<u8> {
    server_key()
        .to_bytes()
        .expect("failed to encode server key to wire format")
}
