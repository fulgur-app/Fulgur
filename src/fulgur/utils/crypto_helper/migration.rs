use super::keychain::{
    device_api_key_user, load_from_keychain, private_key_user, save_or_remove_to_keychain,
};
use super::{DEVICE_API_KEY_PREFIX, PRIVATE_KEY_PREFIX};
use crate::fulgur::settings::Settings;

/// Migrate legacy single-server keychain entries into per-profile entries
/// for the given profile id.
///
/// ### Arguments
/// - `profile_id`: The id of the profile that should receive the migrated
///   credentials (typically the migrated "Fulgurant" profile).
///
/// ### Errors
/// - Returns an error if any of the keychain read, write, or delete operations
///   fail; legacy entries are left in place so the migration can be retried.
///
/// ### Returns
/// - `Ok(())`: Migration completed (or had nothing to migrate).
/// - `Err(anyhow::Error)`: If a keychain operation failed; the legacy entries
///   are left in place so a future startup can retry.
pub fn migrate_legacy_keychain_to_profile(profile_id: &str) -> anyhow::Result<()> {
    if let Some(legacy_private) = load_from_keychain(PRIVATE_KEY_PREFIX)? {
        let target_user = private_key_user(profile_id);
        if load_from_keychain(&target_user)?.is_none() {
            log::info!("Migrating legacy private key to profile '{profile_id}'");
            save_or_remove_to_keychain(&target_user, Some(&legacy_private))?;
        }
        save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, None)?;
    }
    if let Some(legacy_api) = load_from_keychain(DEVICE_API_KEY_PREFIX)? {
        let target_user = device_api_key_user(profile_id);
        if load_from_keychain(&target_user)?.is_none() {
            log::info!("Migrating legacy device API key to profile '{profile_id}'");
            save_or_remove_to_keychain(&target_user, Some(&legacy_api))?;
        }
        save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, None)?;
    }
    Ok(())
}

/// Detect whether legacy single-server keychain entries (`Fulgur:private_key` and/or `Fulgur:device_api_key`) are still present.
///
/// ### Returns
/// - `Ok(true)`: At least one legacy entry exists.
/// - `Ok(false)`: No legacy entries are present.
/// - `Err(anyhow::Error)`: A keychain access failed.
fn legacy_keychain_entries_present() -> anyhow::Result<bool> {
    Ok(load_from_keychain(PRIVATE_KEY_PREFIX)?.is_some()
        || load_from_keychain(DEVICE_API_KEY_PREFIX)?.is_some())
}

/// Migrate legacy single-server keychain entries into the first configured profile, regardless of whether sync is currently activated.
///
/// ### Arguments
/// - `settings`: Application settings used to locate the target profile.
///
/// ### Errors
/// -  Returns an error if any keychain read, write, or delete operation fails.
///
/// ### Returns
/// - `Ok(())`: Migration completed or there was nothing to migrate.
/// - `Err(anyhow::Error)`: A keychain operation failed.
pub fn migrate_legacy_keychain_entries_if_present(settings: &Settings) -> anyhow::Result<()> {
    if !legacy_keychain_entries_present()? {
        return Ok(());
    }
    let Some(target_profile_id) = settings
        .app_settings
        .synchronization_settings
        .profiles
        .first()
        .map(|profile| profile.id.clone())
    else {
        log::warn!(
            "Legacy keychain entries detected but no profiles are configured; leaving entries in place"
        );
        return Ok(());
    };
    log::info!(
        "Migrating legacy keychain entries to profile '{target_profile_id}' (first configured profile)"
    );
    migrate_legacy_keychain_to_profile(&target_profile_id)
}

#[cfg(test)]
mod tests {
    use super::super::keychain::{
        device_api_key_user, load_device_api_key_from_keychain, load_from_keychain,
        load_private_key_from_keychain, private_key_user, save_or_remove_to_keychain,
    };
    use super::super::{DEVICE_API_KEY_PREFIX, PRIVATE_KEY_PREFIX};
    use super::migrate_legacy_keychain_entries_if_present;
    use crate::fulgur::settings::{ServerProfile, Settings};

    /// Serialize migration-related tests so concurrent runs cannot stomp on the
    /// shared legacy keychain entry names.
    fn migration_test_lock() -> &'static std::sync::Mutex<()> {
        use std::sync::OnceLock;
        static LOCK: OnceLock<std::sync::Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| std::sync::Mutex::new(()))
    }

    fn clear_legacy_keychain_entries() {
        let _ = save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, None);
        let _ = save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, None);
    }

    #[test]
    fn test_migrate_legacy_keychain_entries_copies_to_first_profile_and_deletes_legacy() {
        let _guard = migration_test_lock().lock().unwrap();
        clear_legacy_keychain_entries();

        save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, Some("legacy-private")).unwrap();
        save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, Some("legacy-device-key")).unwrap();

        let mut settings = Settings::new();
        let profile = ServerProfile::new("Fulgurant");
        let target_id = profile.id.clone();
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .push(profile);

        migrate_legacy_keychain_entries_if_present(&settings)
            .expect("migration should succeed when legacy entries exist");

        // Legacy entries are gone.
        assert!(
            load_from_keychain(PRIVATE_KEY_PREFIX).unwrap().is_none(),
            "legacy private key entry must be deleted"
        );
        assert!(
            load_from_keychain(DEVICE_API_KEY_PREFIX).unwrap().is_none(),
            "legacy device API key entry must be deleted"
        );
        // Per-profile entries carry the migrated values.
        let migrated_private = load_from_keychain(&private_key_user(&target_id))
            .unwrap()
            .expect("private key must be migrated under the new profile");
        assert_eq!(migrated_private, "legacy-private");
        let migrated_api = load_from_keychain(&device_api_key_user(&target_id))
            .unwrap()
            .expect("device API key must be migrated under the new profile");
        assert_eq!(migrated_api, "legacy-device-key");

        // Cleanup.
        let _ = save_or_remove_to_keychain(&private_key_user(&target_id), None);
        let _ = save_or_remove_to_keychain(&device_api_key_user(&target_id), None);
    }

    #[test]
    fn test_migrate_legacy_keychain_entries_is_noop_when_no_legacy_entries() {
        let _guard = migration_test_lock().lock().unwrap();
        clear_legacy_keychain_entries();

        let mut settings = Settings::new();
        let profile = ServerProfile::new("Fulgurant");
        let target_id = profile.id.clone();
        settings
            .app_settings
            .synchronization_settings
            .profiles
            .push(profile);

        // Pre-state: no entries anywhere for this profile.
        assert!(
            load_private_key_from_keychain(&target_id)
                .unwrap()
                .is_none()
        );
        assert!(
            load_device_api_key_from_keychain(&target_id)
                .unwrap()
                .is_none()
        );

        migrate_legacy_keychain_entries_if_present(&settings)
            .expect("migration without legacy entries must be a no-op");

        // No new entries should have been created.
        assert!(
            load_private_key_from_keychain(&target_id)
                .unwrap()
                .is_none(),
            "no per-profile private key should be created when legacy is absent"
        );
        assert!(
            load_device_api_key_from_keychain(&target_id)
                .unwrap()
                .is_none(),
            "no per-profile device API key should be created when legacy is absent"
        );
    }

    #[test]
    fn test_migrate_legacy_keychain_entries_warns_and_skips_when_no_profiles() {
        let _guard = migration_test_lock().lock().unwrap();
        clear_legacy_keychain_entries();

        save_or_remove_to_keychain(PRIVATE_KEY_PREFIX, Some("orphan-private")).unwrap();
        save_or_remove_to_keychain(DEVICE_API_KEY_PREFIX, Some("orphan-device-key")).unwrap();

        let settings = Settings::new(); // no profiles configured
        migrate_legacy_keychain_entries_if_present(&settings)
            .expect("migration must succeed even when there are no profiles");

        // Legacy entries must be left in place so a later activation can recover them.
        assert_eq!(
            load_from_keychain(PRIVATE_KEY_PREFIX).unwrap().as_deref(),
            Some("orphan-private"),
            "legacy private key must be preserved when no profile exists to receive it"
        );
        assert_eq!(
            load_from_keychain(DEVICE_API_KEY_PREFIX)
                .unwrap()
                .as_deref(),
            Some("orphan-device-key"),
            "legacy device API key must be preserved when no profile exists to receive it"
        );

        // Cleanup so other tests start fresh.
        clear_legacy_keychain_entries();
    }
}
