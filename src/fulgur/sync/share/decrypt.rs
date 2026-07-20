use crate::fulgur::settings::ServerProfile;
use crate::fulgur::shared_state::SyncState;
use crate::fulgur::sync::share;
use crate::fulgur::utils::crypto_helper::{self, load_private_key_from_keychain};
use gpui::SharedString;
use gpui_component::notification::NotificationType;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::{Duration, Instant};

/// Maximum per-share decryption attempts before the share is quarantined.
const MAX_DECRYPT_ATTEMPTS: u32 = 5;

/// Base delay for the exponential decryption-retry backoff (doubles per attempt).
const DECRYPT_RETRY_BASE_DELAY: Duration = Duration::from_secs(2);

/// Fixed retry delay applied when the private key cannot be loaded from the  keychain.
const KEY_RETRY_DELAY: Duration = Duration::from_secs(5);

/// A shared file that has been decrypted and decompressed into plaintext,
/// ready for the render loop to open as a new tab.
pub struct DecryptedShare {
    pub file_name: String,
    pub content: String,
}

/// Retry bookkeeping for a pending share that failed to decode.
pub struct ShareRetryState {
    /// Number of failed decryption attempts so far.
    pub attempts: u32,
    /// Earliest time the next decryption attempt may run.
    pub next_retry_at: Instant,
}

/// Compute the exponential backoff delay for a given failed-attempt count.
///
/// ### Arguments
/// - `attempts`: The number of failed decryption attempts so far (at least 1).
///
/// ### Returns
/// - `Duration`: `DECRYPT_RETRY_BASE_DELAY * 2^(attempts - 1)`.
fn backoff_delay(attempts: u32) -> Duration {
    DECRYPT_RETRY_BASE_DELAY.saturating_mul(1u32 << (attempts.saturating_sub(1)).min(16))
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
    /// Number of files dropped after exhausting the decryption attempt cap.
    quarantined_count: usize,
}

/// Start a one-shot background decryption pass for a profile when encrypted
/// shares are due for an attempt and no pass is already running.
///
/// Shares in their retry-backoff window do not trigger a pass, so a queue
/// holding only failing shares stays cheap to poll from the render loop.
///
/// ### Arguments
/// - `profile`: The server profile whose pending shares should be decoded;
///   also used to acknowledge v2 shares once they decrypt successfully.
/// - `sync_state`: The shared sync state holding the queues and guard flag.
/// - `http_agent`: Shared HTTP agent used to acknowledge downloaded v2 shares.
pub fn start_decryption_if_idle(
    profile: &ServerProfile,
    sync_state: &Arc<SyncState>,
    http_agent: &Arc<ureq::Agent>,
) {
    if !any_share_due(sync_state, Instant::now()) {
        return;
    }
    if sync_state
        .decrypt_in_flight
        .compare_exchange(false, true, Ordering::AcqRel, Ordering::Acquire)
        .is_err()
    {
        return;
    }
    let profile = profile.clone();
    let sync_state = Arc::clone(sync_state);
    let http_agent = Arc::clone(http_agent);
    thread::spawn(move || {
        run_decryption_pass(&profile, &sync_state, &http_agent);
        sync_state.decrypt_in_flight.store(false, Ordering::Release);
    });
}

/// Check whether at least one pending share is outside its retry-backoff window.
///
/// ### Arguments
/// - `sync_state`: The shared sync state holding the queue and retry bookkeeping.
/// - `now`: The instant to compare retry deadlines against.
///
/// ### Returns
/// - `true`: At least one pending share is due for a decryption attempt.
/// - `false`: The queue is empty or every pending share is backed off.
fn any_share_due(sync_state: &SyncState, now: Instant) -> bool {
    let pending = sync_state.pending_shared_files.lock();
    if pending.is_empty() {
        return false;
    }
    let retry_state = sync_state.share_retry_state.lock();
    pending.iter().any(|share| {
        retry_state
            .get(&share.id)
            .is_none_or(|retry| retry.next_retry_at <= now)
    })
}

/// Decode every currently due pending share for a profile on a background thread.
///
/// ### Arguments
/// - `profile`: The server profile whose shares are being decoded; also used to
///   acknowledge v2 shares that decrypt successfully.
/// - `sync_state`: The shared sync state holding the queues and notification slots.
/// - `http_agent`: Shared HTTP agent used to acknowledge downloaded v2 shares.
fn run_decryption_pass(profile: &ServerProfile, sync_state: &SyncState, http_agent: &ureq::Agent) {
    let profile_id = &profile.id;
    let profile_name = profile.name.as_str();
    let server_max_size = sync_state.max_file_size_bytes.load(Ordering::Acquire);
    let now = Instant::now();
    let shared_files: Vec<fulgur_common::api::shares::SharedFileResponse> = {
        let mut pending = sync_state.pending_shared_files.lock();
        let retry_state = sync_state.share_retry_state.lock();
        let (due, deferred): (Vec<_>, Vec<_>) = std::mem::take(&mut *pending)
            .into_iter()
            .partition(|share| {
                retry_state
                    .get(&share.id)
                    .is_none_or(|retry| retry.next_retry_at <= now)
            });
        *pending = deferred;
        due
    };
    if shared_files.is_empty() {
        return;
    }

    let mut retry_queue = Vec::new();
    let mut outcome = DecryptionOutcome::default();
    let mut decrypted_ids: Vec<String> = Vec::new();

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
                        sync_state.share_retry_state.lock().remove(&shared_file.id);
                        outcome.decrypted_count += 1;
                        decrypted_ids.push(shared_file.id.clone());
                        log::info!("Decrypted shared file: {}", shared_file.file_name);
                    }
                    Err(e) => {
                        let attempts = {
                            let mut retry_state = sync_state.share_retry_state.lock();
                            let entry = retry_state.entry(shared_file.id.clone()).or_insert(
                                ShareRetryState {
                                    attempts: 0,
                                    next_retry_at: now,
                                },
                            );
                            entry.attempts += 1;
                            entry.next_retry_at = Instant::now() + backoff_delay(entry.attempts);
                            entry.attempts
                        };
                        if attempts >= MAX_DECRYPT_ATTEMPTS {
                            sync_state.share_retry_state.lock().remove(&shared_file.id);
                            outcome.quarantined_count += 1;
                            log::warn!(
                                "Quarantining shared file '{}' for profile {profile_id}: decryption failed {attempts} time(s), giving up ({e}). \
                                 The share was likely encrypted for a different device and will expire server-side.",
                                shared_file.file_name
                            );
                        } else {
                            outcome.decrypt_failures += 1;
                            log::warn!(
                                "Deferring shared file '{}' for profile {profile_id}: decryption failed ({e}), attempt {attempts} of {MAX_DECRYPT_ATTEMPTS}",
                                shared_file.file_name
                            );
                            retry_queue.push(shared_file);
                        }
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
            defer_batch_for_key_retry(sync_state, &shared_files);
            retry_queue.extend(shared_files);
        }
        Err(e) => {
            outcome.key_load_failed = true;
            log::warn!(
                "Deferring {} shared file(s) for profile {profile_id}: failed to load encryption key from keychain: {e}",
                shared_files.len()
            );
            defer_batch_for_key_retry(sync_state, &shared_files);
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

    acknowledge_downloaded_shares(profile, sync_state, http_agent, &decrypted_ids);

    outcome.publish_notification(profile_name, sync_state);
}

/// Push every share's next retry out by `KEY_RETRY_DELAY` after a keychain
/// failure, without counting a decryption attempt.
///
/// ### Arguments
/// - `sync_state`: The shared sync state holding the retry bookkeeping.
/// - `shared_files`: The batch of shares deferred by the keychain failure.
fn defer_batch_for_key_retry(
    sync_state: &SyncState,
    shared_files: &[fulgur_common::api::shares::SharedFileResponse],
) {
    let next_retry_at = Instant::now() + KEY_RETRY_DELAY;
    let mut retry_state = sync_state.share_retry_state.lock();
    for share in shared_files {
        retry_state
            .entry(share.id.clone())
            .and_modify(|retry| retry.next_retry_at = next_retry_at)
            .or_insert(ShareRetryState {
                attempts: 0,
                next_retry_at,
            });
    }
}

/// Acknowledge successfully-downloaded v2 shares, consuming them server-side.
///
/// ### Arguments
/// - `profile`: The server profile to acknowledge against.
/// - `sync_state`: The shared sync state holding the ack set and token manager.
/// - `http_agent`: Shared HTTP agent for connection pooling.
/// - `decrypted_ids`: IDs of shares decrypted successfully in this pass.
fn acknowledge_downloaded_shares(
    profile: &ServerProfile,
    sync_state: &SyncState,
    http_agent: &ureq::Agent,
    decrypted_ids: &[String],
) {
    if decrypted_ids.is_empty() {
        return;
    }
    let ids_to_ack: Vec<String> = {
        let pending_ack = sync_state.pending_ack_share_ids.lock();
        decrypted_ids
            .iter()
            .filter(|id| pending_ack.contains(*id))
            .cloned()
            .collect()
    };
    for id in ids_to_ack {
        match share::acknowledge_share_download(profile, &sync_state.token_state, http_agent, &id) {
            Ok(()) => {
                sync_state.pending_ack_share_ids.lock().remove(&id);
            }
            Err(e) => {
                log::warn!(
                    "Failed to acknowledge downloaded share {id}: {e}; duplicate delivery is suppressed and the share will expire server-side"
                );
            }
        }
    }
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
        } else if self.quarantined_count > 0 {
            Some((
                "share-decryption-quarantined",
                SharedString::from(format!(
                    "{profile_name}: Could not decrypt {} shared file(s) after {MAX_DECRYPT_ATTEMPTS} attempts. They may have been encrypted for a different device and will not be retried.",
                    self.quarantined_count
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
                sync_state.notify((NotificationType::Error, message));
                *last_signature = Some(signature.to_string());
            }
        } else if self.decrypted_count > 0 || self.retry_count == 0 {
            *sync_state.last_share_receive_error_signature.lock() = None;
        }
    }
}
