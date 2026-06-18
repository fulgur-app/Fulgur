use parking_lot::Mutex;
use std::{
    collections::HashMap,
    ffi::OsStr,
    sync::{
        Once, OnceLock,
        atomic::{AtomicBool, Ordering},
    },
};

use super::SERVICE_NAME;

/// Registers the platform credential store as the `keyring-core` default.
///
/// ### Errors
/// - Returns an error if the platform store cannot be created or the current
///   platform has no supported native store.
///
/// ### Returns
/// - `Ok(())`: The default store was registered.
/// - `Err(anyhow::Error)`: The store could not be created or is unsupported.
#[cfg(target_os = "macos")]
fn register_platform_store() -> anyhow::Result<()> {
    let store = apple_native_keyring_store::keychain::Store::new()
        .map_err(|e| anyhow::anyhow!("Failed to create macOS keychain store: {e}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// See the macOS variant of [`register_platform_store`].
#[cfg(target_os = "windows")]
fn register_platform_store() -> anyhow::Result<()> {
    let store = windows_native_keyring_store::Store::new()
        .map_err(|e| anyhow::anyhow!("Failed to create Windows credential store: {e}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// See the macOS variant of [`register_platform_store`].
#[cfg(target_os = "linux")]
fn register_platform_store() -> anyhow::Result<()> {
    let store = dbus_secret_service_keyring_store::store::Store::new()
        .map_err(|e| anyhow::anyhow!("Failed to create Linux Secret Service store: {e}"))?;
    keyring_core::set_default_store(store);
    Ok(())
}

/// See the macOS variant of [`register_platform_store`].
#[cfg(not(any(target_os = "macos", target_os = "windows", target_os = "linux")))]
fn register_platform_store() -> anyhow::Result<()> {
    Err(anyhow::anyhow!(
        "No native keychain backend is available for this platform"
    ))
}

/// Ensures the platform credential store is registered exactly once.
///
/// ### Errors
/// - Returns an error if the platform store could not be registered.
///
/// ### Returns
/// - `Ok(())`: The default store is registered and ready.
/// - `Err(anyhow::Error)`: The default store could not be registered.
pub fn init_keychain_backend() -> anyhow::Result<()> {
    static STORE_INIT: Once = Once::new();
    static INIT_OK: AtomicBool = AtomicBool::new(false);
    STORE_INIT.call_once(|| match register_platform_store() {
        Ok(()) => INIT_OK.store(true, Ordering::SeqCst),
        Err(e) => log::error!("Failed to register keychain backend: {e}"),
    });
    if INIT_OK.load(Ordering::SeqCst) {
        Ok(())
    } else {
        Err(anyhow::anyhow!("Keychain backend is not available"))
    }
}

/// Checks whether an environment variable contains a truthy value.
///
/// Accepted truthy values are: `1`, `true`, `yes`, `on` (case-insensitive).
///
/// ### Arguments
/// - `name`: The environment variable name to evaluate.
///
/// ### Returns
/// - `true`: If the variable exists and is set to a recognized truthy value.
/// - `false`: Otherwise.
fn env_var_is_truthy(name: &str) -> bool {
    matches!(
        std::env::var(name)
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .as_deref(),
        Some("1" | "true" | "yes" | "on")
    )
}

/// Determines whether keychain operations should use the in-memory backend.
///
/// This avoids interactive keychain prompts during `cargo test` and in CI.
/// Set `FULGUR_USE_REAL_KEYCHAIN=1` to force real keychain access.
///
/// Precedence:
/// 1. `FULGUR_USE_REAL_KEYCHAIN=1` always forces the real keychain backend.
/// 2. `FULGUR_DISABLE_KEYCHAIN=1` forces the in-memory backend.
/// 3. `CI=1` forces the in-memory backend.
/// 4. Test binary heuristics (`target/*/deps/*-<hash>`) use the in-memory backend.
///
/// ### Returns
/// - `true`: Use in-memory keychain storage.
/// - `false`: Use the platform keychain backend.
pub(super) fn should_use_in_memory_keychain() -> bool {
    if env_var_is_truthy("FULGUR_USE_REAL_KEYCHAIN") {
        return false;
    }
    if env_var_is_truthy("FULGUR_DISABLE_KEYCHAIN") {
        return true;
    }
    if env_var_is_truthy("CI") {
        return true;
    }
    // `cargo test` binaries are typically emitted under `target/*/deps/`.
    if let Ok(exe) = std::env::current_exe() {
        let in_deps_dir = exe.parent().is_some_and(|parent| parent.ends_with("deps"));
        let has_hashed_test_name = exe
            .file_name()
            .and_then(OsStr::to_str)
            .is_some_and(|name| name.contains('-'));
        if in_deps_dir && has_hashed_test_name {
            return true;
        }
    }
    false
}

/// Returns the process-local in-memory keychain store.
///
/// ### Returns
/// - `&'static Mutex<HashMap<String, String>>`: Shared in-memory credential store.
fn in_memory_keychain() -> &'static Mutex<HashMap<String, String>> {
    static STORE: OnceLock<Mutex<HashMap<String, String>>> = OnceLock::new();
    STORE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Builds the in-memory storage key for a keychain user entry.
///
/// ### Arguments
/// - `user`: The keychain entry name (for example, `private_key`).
///
/// ### Returns
/// - `String`: A namespaced key using the service name and user.
fn in_memory_key(user: &str) -> String {
    format!("{SERVICE_NAME}:{user}")
}

/// Saves or removes a value in the in-memory keychain backend.
///
/// ### Arguments
/// - `user`: The keychain entry name.
/// - `value`: The value to save.
pub(super) fn save_or_remove_to_in_memory_keychain(user: &str, value: Option<&str>) {
    let mut keychain = in_memory_keychain().lock();
    let key = in_memory_key(user);
    if let Some(value) = value
        && !value.is_empty()
    {
        keychain.insert(key, value.to_string());
    } else {
        keychain.remove(&key);
    }
}

/// Loads a value from the in-memory keychain backend.
///
/// ### Arguments
/// - `user`: The keychain entry name.
///
/// ### Returns
/// - `Some(String)`: The value exists.
/// - `None`: The value does not exist.
pub(super) fn load_from_in_memory_keychain(user: &str) -> Option<String> {
    let keychain = in_memory_keychain().lock();
    keychain.get(&in_memory_key(user)).cloned()
}
