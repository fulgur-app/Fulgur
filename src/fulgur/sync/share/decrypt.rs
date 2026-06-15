use crate::fulgur::settings::ProfileId;
use crate::fulgur::shared_state::SyncState;
use crate::fulgur::sync::share;
use crate::fulgur::utils::crypto_helper::{self, load_private_key_from_keychain};
use gpui::SharedString;
use gpui_component::notification::NotificationType;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;

/// A shared file that has been decrypted and decompressed into plaintext,
/// ready for the render loop to open as a new tab.
pub struct DecryptedShare {
    pub file_name: String,
    pub content: String,
}

/// Tallies collected during a single decryption pass, used to decide which
/// deduplicated error notification (if any) to surface to the user.
#[derive(Default)]
struct DecryptionOutcome {
    /// The keychain held no private key entry for the profile.
    key_unavailable: bool,
    /// Loading the private key from the keychain errored.
    key_load_failed: bool,
    /// Number of files that failed to decode this pass.
    decrypt_failures: usize,
    /// Number of files decoded successfully this pass.
    decrypted_count: usize,
    /// Number of files re-queued for a later attempt.
    retry_count: usize,
}

/// Start a one-shot background decryption pass for a profile when encrypted
/// shares are waiting and no pass is already running.
///
/// ### Arguments
/// - `profile_id`: The profile whose pending shares should be decoded.
/// - `profile_name`: The human-readable profile name used in notifications.
/// - `sync_state`: The shared sync state holding the queues and guard flag.
pub fn start_decryption_if_idle(
    profile_id: &ProfileId,
    profile_name: &str,
    sync_state: &Arc<SyncState>,
) {
    if sync_state.pending_shared_files.lock().is_empty() {
        return;
    }
    if sync_state
        .decrypt_in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let profile_id = profile_id.clone();
    let profile_name = profile_name.to_string();
    let sync_state = Arc::clone(sync_state);
    thread::spawn(move || {
        run_decryption_pass(&profile_id, &profile_name, &sync_state);
        sync_state.decrypt_in_flight.store(false, Ordering::Release);
    });
}

/// Decode every currently pending share for a profile on a background thread.
///
/// ### Arguments
/// - `profile_id`: The profile whose shares are being decoded.
/// - `profile_name`: The human-readable profile name used in notifications.
/// - `sync_state`: The shared sync state holding the queues and notification slots.
fn run_decryption_pass(profile_id: &ProfileId, profile_name: &str, sync_state: &SyncState) {
    let server_max_size = sync_state.max_file_size_bytes.load(Ordering::Acquire);
    let shared_files: Vec<fulgur_common::api::shares::SharedFileResponse> = {
        let mut pending = sync_state.pending_shared_files.lock();
        std::mem::take(&mut *pending)
    };
    if shared_files.is_empty() {
        return;
    }

    let mut retry_queue = Vec::new();
    let mut outcome = DecryptionOutcome::default();

    match load_private_key_from_keychain(profile_id) {
        Ok(Some(encryption_key)) => {
            for shared_file in shared_files {
                if server_max_size != u64::MAX
                    && shared_file.content.len() as u64 > server_max_size.saturating_mul(2)
                {
                    log::warn!(
                        "Skipping shared file '{}' from device {}: encrypted payload ({} bytes) exceeds the server max ({} bytes)",
                        shared_file.file_name,
                        shared_file.source_device_id,
                        shared_file.content.len(),
                        server_max_size
                    );
                    continue;
                }

                let decoded =
                    crypto_helper::decrypt_bytes(&shared_file.content, encryption_key.as_str())
                        .and_then(|compressed_bytes| {
                            share::decompress_content(&compressed_bytes, server_max_size)
                        });

                match decoded {
                    Ok(content) => {
                        sync_state
                            .pending_decrypted_files
                            .lock()
                            .push(DecryptedShare {
                                file_name: shared_file.file_name.clone(),
                                content,
                            });
                        outcome.decrypted_count += 1;
                        log::info!("Decrypted shared file: {}", shared_file.file_name);
                    }
                    Err(e) => {
                        outcome.decrypt_failures += 1;
                        log::warn!(
                            "Deferring shared file '{}' for profile {profile_id}: decryption failed ({e})",
                            shared_file.file_name
                        );
                        retry_queue.push(shared_file);
                    }
                }
            }
            drop(encryption_key);
        }
        Ok(None) => {
            outcome.key_unavailable = true;
            log::warn!(
                "Deferring {} shared file(s) for profile {profile_id}: encryption key is unavailable",
                shared_files.len()
            );
            retry_queue.extend(shared_files);
        }
        Err(e) => {
            outcome.key_load_failed = true;
            log::warn!(
                "Deferring {} shared file(s) for profile {profile_id}: failed to load encryption key from keychain: {e}",
                shared_files.len()
            );
            retry_queue.extend(shared_files);
        }
    }

    outcome.retry_count = retry_queue.len();
    if outcome.retry_count > 0 {
        let mut pending = sync_state.pending_shared_files.lock();
        retry_queue.extend(std::mem::take(&mut *pending));
        *pending = retry_queue;
        log::warn!(
            "Re-queued {} shared file(s) for profile {profile_id} for retry",
            outcome.retry_count
        );
    }

    outcome.publish_notification(profile_name, sync_state);
}

impl DecryptionOutcome {
    /// Publish or clear the deduplicated share-receive error notification for this pass.
    ///
    /// ### Arguments
    /// - `profile_name`: The human-readable profile name used in messages.
    /// - `sync_state`: The shared sync state holding the notification slots.
    fn publish_notification(&self, profile_name: &str, sync_state: &SyncState) {
        let error_notification = if self.key_unavailable {
            Some((
                "missing-keychain-private-key",
                SharedString::from(format!(
                    "{profile_name}: Cannot receive shared files because the encryption key is unavailable in the keychain. Fulgur will retry automatically."
                )),
            ))
        } else if self.key_load_failed {
            Some((
                "failed-to-load-keychain-private-key",
                SharedString::from(format!(
                    "{profile_name}: Cannot receive shared files because the encryption key could not be loaded from the keychain. Fulgur will retry automatically."
                )),
            ))
        } else if self.decrypt_failures > 0 {
            Some((
                "share-decryption-failed",
                SharedString::from(format!(
                    "{profile_name}: Failed to decrypt {} shared file(s). Fulgur will retry automatically.",
                    self.decrypt_failures
                )),
            ))
        } else {
            None
        };

        if let Some((signature, message)) = error_notification {
            let mut last_signature = sync_state.last_share_receive_error_signature.lock();
            if last_signature.as_deref() != Some(signature) {
                *sync_state.pending_notification.lock() = Some((NotificationType::Error, message));
                *last_signature = Some(signature.to_string());
            }
        } else if self.decrypted_count > 0 || self.retry_count == 0 {
            *sync_state.last_share_receive_error_signature.lock() = None;
        }
    }
}
