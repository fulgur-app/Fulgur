use crate::fulgur::{
    Fulgur,
    settings::ServerProfile,
    utils::crypto_helper::{
        ensure_profile_keypair, save_device_api_key_to_keychain, save_private_key_to_keychain,
    },
};
use gpui::Context;

impl Fulgur {
    /// Add a new server profile to the configuration.
    ///
    /// ### Arguments
    /// - `profile`: The fully formed profile to register.
    /// - `cx`: The Fulgur context.
    ///
    /// ### Errors
    /// Returns an error if persisting the updated settings fails.
    ///
    /// ### Returns
    /// - `Ok(())`: The profile was added and settings persisted.
    /// - `Err(anyhow::Error)`: The settings could not be saved; the in-memory
    ///   profile list is left mutated but the on-disk store is unchanged.
    pub fn add_profile(
        &mut self,
        mut profile: ServerProfile,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<()> {
        if profile.is_active
            && let Err(e) = ensure_profile_keypair(&mut profile)
        {
            log::error!(
                "Failed to ensure keypair for new profile '{}': {e}",
                profile.name
            );
        }
        let profile_id = profile.id.clone();
        self.settings
            .app_settings
            .synchronization_settings
            .profiles
            .push(profile);
        let _ = Fulgur::shared_state(cx).sync_state_for(&profile_id);
        self.update_and_propagate_settings(cx)
    }

    /// Update an existing profile and propagate the change.
    ///
    /// ### Arguments
    /// - `profile_id`: The id of the profile to mutate.
    /// - `mutator`: Closure invoked with the mutable profile reference.
    /// - `cx`: The Fulgur context.
    ///
    /// ### Errors
    /// Returns an error if persisting the updated settings fails.
    ///
    /// ### Returns
    /// - `Ok(true)`: The profile was found, mutated, and settings persisted.
    /// - `Ok(false)`: No profile with the given id exists.
    /// - `Err(anyhow::Error)`: Persistence failed.
    pub fn update_profile<F>(
        &mut self,
        profile_id: &str,
        mutator: F,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<bool>
    where
        F: FnOnce(&mut ServerProfile),
    {
        let found = self
            .settings
            .app_settings
            .synchronization_settings
            .find_profile_mut(profile_id)
            .is_some_and(|profile| {
                mutator(profile);
                true
            });
        if !found {
            return Ok(false);
        }

        if let Some(profile) = self
            .settings
            .app_settings
            .synchronization_settings
            .find_profile_mut(profile_id)
            && profile.is_active
            && let Err(e) = ensure_profile_keypair(profile)
        {
            log::error!(
                "Failed to ensure keypair for profile '{}': {e}",
                profile.name
            );
        }
        self.update_and_propagate_settings(cx)?;
        Ok(true)
    }

    /// Delete a server profile and tear down its associated state.
    ///
    /// ### Arguments
    /// - `profile_id`: The id of the profile to delete.
    /// - `cx`: The Fulgur context.
    ///
    /// ### Errors
    /// Returns an error if persisting the updated settings fails.
    ///
    /// ### Returns
    /// - `Ok(true)`: The profile was found and removed.
    /// - `Ok(false)`: No profile with the given id exists.
    /// - `Err(anyhow::Error)`: Persistence failed; in-memory state is still
    ///   updated, on-disk settings may diverge until the next save.
    pub fn delete_profile(
        &mut self,
        profile_id: &str,
        cx: &mut Context<Self>,
    ) -> anyhow::Result<bool> {
        let exists = self
            .settings
            .app_settings
            .synchronization_settings
            .find_profile(profile_id)
            .is_some();
        if !exists {
            return Ok(false);
        }
        if let Some(ref shutdown_flag) = Fulgur::shared_state(cx)
            .sync_state_for(profile_id)
            .sse
            .lock()
            .sse_shutdown_flag
        {
            shutdown_flag.store(true, std::sync::atomic::Ordering::Relaxed);
        }
        if let Err(e) = save_private_key_to_keychain(profile_id, None) {
            log::warn!("Failed to remove private key for profile '{profile_id}': {e}");
        }
        if let Err(e) = save_device_api_key_to_keychain(profile_id, None) {
            log::warn!("Failed to remove device API key for profile '{profile_id}': {e}");
        }
        let _ = Fulgur::shared_state(cx).remove_sync_state(profile_id);
        self.settings
            .app_settings
            .synchronization_settings
            .profiles
            .retain(|p| p.id != profile_id);
        self.update_and_propagate_settings(cx)?;
        Ok(true)
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{
        Fulgur,
        settings::{ServerProfile, Settings},
        shared_state::SharedAppState,
        utils::crypto_helper::{
            load_device_api_key_from_keychain, load_private_key_from_keychain,
            save_device_api_key_to_keychain, save_private_key_to_keychain,
        },
        window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};
    use zeroize::Zeroizing;

    /// Initialize globals and open a test window with a Root-mounted Fulgur.
    fn setup_fulgur(cx: &mut TestAppContext) -> (Entity<Fulgur>, VisualTestContext) {
        cx.update(|cx| {
            gpui_component::init(cx);
            let mut settings = Settings::new();
            settings.editor_settings.watch_files = false;
            let pending_files: Arc<Mutex<Vec<PathBuf>>> = Arc::new(Mutex::new(Vec::new()));
            cx.set_global(SharedAppState::new(settings, pending_files));
            cx.set_global(WindowManager::new());
        });
        let fulgur_slot: RefCell<Option<Entity<Fulgur>>> = RefCell::new(None);
        let window = cx
            .update(|cx| {
                cx.open_window(WindowOptions::default(), |window, cx| {
                    let window_id = window.window_handle().window_id();
                    let fulgur = Fulgur::new(window, cx, window_id, usize::MAX);
                    *fulgur_slot.borrow_mut() = Some(fulgur.clone());
                    cx.new(|cx| gpui_component::Root::new(fulgur, window, cx))
                })
            })
            .expect("failed to open test window");
        let visual_cx = VisualTestContext::from_window(window.into(), cx);
        visual_cx.run_until_parked();
        let fulgur = fulgur_slot
            .into_inner()
            .expect("failed to capture Fulgur entity");
        (fulgur, visual_cx)
    }

    #[gpui::test]
    fn test_add_profile_inserts_and_allocates_sse_slot(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("Added");
                let id = profile.id.clone();
                this.add_profile(profile, cx).expect("add should persist");
                assert!(
                    this.settings
                        .app_settings
                        .synchronization_settings
                        .find_profile(&id)
                        .is_some(),
                    "profile must be present in settings"
                );
                assert!(
                    Fulgur::shared_state(cx)
                        .sync_states
                        .read()
                        .contains_key(&id),
                    "shared sync state (with SSE channel) must exist for the new profile"
                );
            });
        });
    }

    #[gpui::test]
    fn test_add_profile_generates_keypair_for_active_profile(cx: &mut TestAppContext) {
        // Reproduces the user-reported "Missing encryption key" path: a
        // freshly added active profile must have its keypair generated as
        // part of `add_profile`, otherwise `initial_synchronization` bails
        // out before any network call.
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let mut profile = ServerProfile::new("Active-On-Add");
                profile.is_active = true;
                let id = profile.id.clone();
                assert!(profile.public_key.is_none());
                this.add_profile(profile, cx).expect("add should persist");

                let stored = this
                    .settings
                    .app_settings
                    .synchronization_settings
                    .find_profile(&id)
                    .expect("profile should be in settings after add");
                assert!(
                    stored.public_key.is_some(),
                    "public_key must be set on an active profile after add_profile"
                );
                assert!(
                    load_private_key_from_keychain(&id)
                        .expect("keychain access should succeed")
                        .is_some(),
                    "private key must be persisted in keychain for an active profile"
                );
                // Cleanup: drop the keychain entry so other tests start fresh.
                let _ = save_private_key_to_keychain(&id, None);
            });
        });
    }

    #[gpui::test]
    fn test_add_profile_skips_keypair_generation_for_inactive_profile(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("Inactive-On-Add");
                let id = profile.id.clone();
                assert!(!profile.is_active);
                this.add_profile(profile, cx).expect("add should persist");
                let stored = this
                    .settings
                    .app_settings
                    .synchronization_settings
                    .find_profile(&id)
                    .expect("profile should be in settings");
                assert!(
                    stored.public_key.is_none(),
                    "no public_key should be set for an inactive profile"
                );
                assert!(
                    load_private_key_from_keychain(&id)
                        .expect("keychain access should succeed")
                        .is_none(),
                    "no private key should be in keychain for an inactive profile"
                );
            });
        });
    }

    #[gpui::test]
    fn test_update_profile_generates_keypair_when_activating(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("Toggled");
                let id = profile.id.clone();
                this.add_profile(profile, cx).expect("add should persist");
                assert!(
                    load_private_key_from_keychain(&id)
                        .expect("keychain access should succeed")
                        .is_none(),
                    "inactive profile should not yet have a private key"
                );

                let updated = this
                    .update_profile(&id, |existing| existing.is_active = true, cx)
                    .expect("update should persist");
                assert!(updated);

                let stored = this
                    .settings
                    .app_settings
                    .synchronization_settings
                    .find_profile(&id)
                    .expect("profile should still be in settings");
                assert!(
                    stored.public_key.is_some(),
                    "public_key must be generated when a profile is activated via update"
                );
                assert!(
                    load_private_key_from_keychain(&id)
                        .expect("keychain access should succeed")
                        .is_some(),
                    "private key must be persisted in keychain after activation"
                );
                // Cleanup.
                let _ = save_private_key_to_keychain(&id, None);
            });
        });
    }

    #[gpui::test]
    fn test_update_profile_returns_false_for_unknown_id(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let updated = this
                    .update_profile(
                        "missing-profile-id",
                        |profile| profile.name = "Other".into(),
                        cx,
                    )
                    .expect("update without persistence should not error");
                assert!(!updated, "unknown id must return false");
            });
        });
    }

    #[gpui::test]
    fn test_update_profile_mutates_existing_entry(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("Original");
                let id = profile.id.clone();
                this.add_profile(profile, cx).expect("add should persist");
                let updated = this
                    .update_profile(
                        &id,
                        |profile| {
                            profile.name = "Renamed".into();
                            profile.is_active = true;
                        },
                        cx,
                    )
                    .expect("update should persist");
                assert!(updated);
                let stored = this
                    .settings
                    .app_settings
                    .synchronization_settings
                    .find_profile(&id)
                    .expect("profile should still exist");
                assert_eq!(stored.name, "Renamed");
                assert!(stored.is_active);
            });
        });
    }

    #[gpui::test]
    fn test_delete_profile_removes_settings_state_and_keychain(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("Doomed");
                let id = profile.id.clone();
                this.add_profile(profile, cx).expect("add should persist");
                save_device_api_key_to_keychain(&id, Some("secret-token"))
                    .expect("seeding device key should succeed");
                save_private_key_to_keychain(&id, Some(&Zeroizing::new("AGE-FAKE".to_string())))
                    .expect("seeding private key should succeed");
                let _ = Fulgur::shared_state(cx).sync_state_for(&id);
                let removed = this.delete_profile(&id, cx).expect("delete should persist");
                assert!(removed, "delete must return true when the profile existed");
                assert!(
                    this.settings
                        .app_settings
                        .synchronization_settings
                        .find_profile(&id)
                        .is_none(),
                    "profile must be gone from settings"
                );
                assert!(
                    !Fulgur::shared_state(cx)
                        .sync_states
                        .read()
                        .contains_key(&id),
                    "SyncState entry must be removed"
                );
                assert!(
                    load_device_api_key_from_keychain(&id)
                        .expect("keychain access should succeed")
                        .is_none(),
                    "device API key entry must be deleted"
                );
                assert!(
                    load_private_key_from_keychain(&id)
                        .expect("keychain access should succeed")
                        .is_none(),
                    "private key entry must be deleted"
                );
            });
        });
    }

    #[gpui::test]
    fn test_delete_profile_returns_false_for_unknown_id(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|_window, cx| {
            fulgur.update(cx, |this, cx| {
                let removed = this
                    .delete_profile("missing", cx)
                    .expect("delete should not error");
                assert!(!removed, "unknown id must return false");
            });
        });
    }
}
