mod backend;
mod crypto;
mod keychain;
mod migration;

pub use backend::init_keychain_backend;
pub use crypto::{
    check_private_public_keys, decrypt_bytes, encrypt_bytes, ensure_profile_keypair,
    generate_key_pair, is_valid_public_key, serialize,
};
pub use keychain::{
    load_device_api_key_from_keychain, load_private_key_from_keychain,
    save_device_api_key_to_keychain, save_private_key_to_keychain,
};
pub use migration::{
    migrate_legacy_keychain_entries_if_present, migrate_legacy_keychain_to_profile,
};

// Prefixes used to namespace per-profile entries inside the keychain.
const PRIVATE_KEY_PREFIX: &str = "private_key";
const DEVICE_API_KEY_PREFIX: &str = "device_api_key";

const SERVICE_NAME: &str = "Fulgur";
