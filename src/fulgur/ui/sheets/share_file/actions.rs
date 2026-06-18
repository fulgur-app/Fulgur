use super::state::{ProfileFetchState, ShareSheetState};
use crate::fulgur::{
    Fulgur,
    settings::{ProfileId, ServerProfile},
    sync::{
        share::{
            Device, ProfileShareOutcome, ShareFileRequest, format_multi_profile_summary,
            get_devices, share_file,
        },
        synchronization::SynchronizationStatus,
    },
    ui::notifications::progress::{CancelCallback, start_progress},
};
use gpui::{App, Entity, SharedString, Window};
use gpui_component::{WindowExt, notification::NotificationType};
use std::sync::Arc;

/// Snapshot of the file content captured from the active editor tab when Share is pressed. Reused across each per-profile share request.
struct ShareContext {
    content: Arc<str>,
    file_name: String,
    file_path: Option<std::path::PathBuf>,
}

/// Resources required to share to a single profile inside the fan-out.
struct ProfileShareResources {
    profile: ServerProfile,
    devices: Arc<Vec<Device>>,
    device_ids: Vec<String>,
    token_state: Arc<crate::fulgur::sync::access_token::TokenStateManager>,
    max_file_size_bytes: u64,
}

/// Capture the file content snapshot from the active editor tab.
///
/// ### Arguments
/// - `entity`: The Fulgur entity owning the active tab.
/// - `cx`: The application context.
///
/// ### Returns
/// - `ShareContext`: The captured file content, name, and optional file path.
fn capture_share_context(entity: &Entity<Fulgur>, cx: &mut App) -> ShareContext {
    entity.update(cx, |this, cx| {
        let active_tab = this.get_active_editor_tab();
        let content: Arc<str> = active_tab.as_ref().map_or_else(
            || Arc::from(""),
            |tab| Arc::from(tab.content.read(cx).value().as_str()),
        );
        let file_path = active_tab.as_ref().and_then(|tab| tab.file_path().cloned());
        let file_name = file_path
            .as_ref()
            .and_then(|path| path.file_name())
            .and_then(|name| name.to_str())
            .unwrap_or("Untitled")
            .to_string();
        ShareContext {
            content,
            file_name,
            file_path,
        }
    })
}

/// Group selected `(profile_id, device_id)` pairs into per-profile resource bundles.
///
/// ### Arguments
/// - `selected_keys`: The user's selection across all groups.
/// - `state`: Shared sheet state holding per-profile loaded devices.
/// - `entity`: The Fulgur entity (used to read shared state for tokens and limits).
/// - `cx`: The application context.
///
/// ### Returns
/// - `Vec<ProfileShareResources>`: One bundle per profile with at least one selected device.
fn build_profile_share_resources(
    selected_keys: &[(ProfileId, String)],
    state: &Arc<ShareSheetState>,
    entity: &Entity<Fulgur>,
    cx: &mut App,
) -> Vec<ProfileShareResources> {
    let mut bundles: Vec<ProfileShareResources> = Vec::new();
    let map = state.per_profile.read();
    for profile in &state.profiles {
        let device_ids: Vec<String> = selected_keys
            .iter()
            .filter(|(pid, _)| pid == &profile.id)
            .map(|(_, did)| did.clone())
            .collect();
        if device_ids.is_empty() {
            continue;
        }
        let devices = if let Some(ProfileFetchState::Loaded(devices)) = map.get(&profile.id) {
            Arc::clone(devices)
        } else {
            log::warn!(
                "Profile '{}': selection present but no loaded devices, skipping",
                profile.name
            );
            continue;
        };
        let (token_state, max_file_size_bytes) = entity.update(cx, |_, cx| {
            let sync_state = Fulgur::shared_state(cx).sync_state_for(&profile.id);
            (
                Arc::clone(&sync_state.token_state),
                sync_state
                    .max_file_size_bytes
                    .load(std::sync::atomic::Ordering::Acquire),
            )
        });
        bundles.push(ProfileShareResources {
            profile: profile.clone(),
            devices,
            device_ids,
            token_state,
            max_file_size_bytes,
        });
    }
    bundles
}

/// Run a single profile's share request and translate the result into a
/// `ProfileShareOutcome` for aggregation.
///
/// ### Arguments
/// - `resources`: The profile and its selected device subset.
/// - `request`: The shared request payload (content, filename, `file_path`).
/// - `http_agent`: Shared HTTP agent.
///
/// ### Returns
/// - `ProfileShareOutcome`: `Completed` for a per-device dispatch result;
///   `Aborted` when validation prevented any per-device call.
fn execute_profile_share(
    resources: &ProfileShareResources,
    request: &ShareFileRequest,
    http_agent: &ureq::Agent,
) -> ProfileShareOutcome {
    let result = share_file(
        &resources.profile,
        request,
        &resources.devices,
        &resources.token_state,
        http_agent,
        resources.max_file_size_bytes,
    );
    match result {
        Ok(share_result) => ProfileShareOutcome::Completed(share_result),
        Err(e) => {
            log::error!(
                "Profile '{}': share aborted: {e}",
                resources.profile.name.as_str()
            );
            ProfileShareOutcome::Aborted(e)
        }
    }
}

/// Pick the right notification type for an aggregated multi-profile share.
///
/// ### Arguments
/// - `outcomes`: Per-profile outcomes after fan-out.
///
/// ### Returns
/// - `NotificationType`: `Success` if every profile completely succeeded,
///   `Error` if no profile had any success, otherwise `Warning`.
fn aggregate_notification_type(outcomes: &[(String, ProfileShareOutcome)]) -> NotificationType {
    let any_success = outcomes.iter().any(|(_, o)| o.has_success());
    let any_failure = outcomes.iter().any(|(_, o)| o.has_failure());
    match (any_success, any_failure) {
        (true, false) => NotificationType::Success,
        (true, true) => NotificationType::Warning,
        _ => NotificationType::Error,
    }
}

/// Handle the Share button: validate selection, fan out per profile, and queue an aggregated notification when every profile has reported back.
///
/// ### Arguments
/// - `state`: Shared sheet state.
/// - `entity`: The Fulgur entity.
/// - `window`: The current window context.
/// - `cx`: The application context.
pub(super) fn handle_share_file(
    state: &Arc<ShareSheetState>,
    entity: &Entity<Fulgur>,
    window: &mut Window,
    cx: &mut App,
) {
    let keys = state.selected.lock().clone();
    if keys.is_empty() {
        window.push_notification(
            (
                NotificationType::Warning,
                SharedString::from("Please select at least one device to share with."),
            ),
            cx,
        );
        return;
    }
    let bundles = build_profile_share_resources(&keys, state, entity, cx);
    if bundles.is_empty() {
        log::warn!("Share aborted: no loaded profiles in selection");
        window.push_notification(
            (
                NotificationType::Warning,
                SharedString::from(
                    "No devices available to share with. Wait for the device list to load and try again.",
                ),
            ),
            cx,
        );
        return;
    }
    let share_context = capture_share_context(entity, cx);
    let http_agent = entity.update(cx, |_, cx| Arc::clone(&Fulgur::shared_state(cx).http_agent));
    let pending_notification = entity.update(cx, |_, cx| {
        let first_profile_id = bundles
            .first()
            .map(|b| b.profile.id.clone())
            .unwrap_or_default();
        Arc::clone(
            &Fulgur::shared_state(cx)
                .sync_state_for(&first_profile_id)
                .pending_notification,
        )
    });
    state
        .active
        .store(false, std::sync::atomic::Ordering::Release);
    entity.update(cx, |this, _| {
        this.share_sheet_state = None;
    });
    window.close_sheet(cx);
    let progress_label = format!("Sharing {}...", share_context.file_name);
    let cancel_callback: Option<CancelCallback> = Some(Box::new(|_, _| {}));
    let progress = start_progress(window, cx, progress_label.into(), cancel_callback);
    let cancel_flag = progress.cancel_flag();
    std::thread::spawn(move || {
        let _progress = progress;
        let outcomes: Vec<(String, ProfileShareOutcome)> = std::thread::scope(|scope| {
            let handles: Vec<_> = bundles
                .iter()
                .map(|bundle| {
                    let request = ShareFileRequest {
                        content: Arc::clone(&share_context.content),
                        file_name: share_context.file_name.clone(),
                        device_ids: bundle.device_ids.clone(),
                        file_path: share_context.file_path.clone(),
                    };
                    let http_agent_ref = &http_agent;
                    scope.spawn(move || {
                        let outcome = execute_profile_share(bundle, &request, http_agent_ref);
                        (bundle.profile.name.clone(), outcome)
                    })
                })
                .collect();
            handles
                .into_iter()
                .map(|h| {
                    h.join().unwrap_or_else(|e| {
                        log::error!("Per-profile share thread panicked: {e:?}");
                        (
                            String::from("(unknown)"),
                            ProfileShareOutcome::Aborted(
                                crate::fulgur::sync::synchronization::SynchronizationError::Other(
                                    "Internal issue".to_string(),
                                ),
                            ),
                        )
                    })
                })
                .collect()
        });
        if cancel_flag.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        let summary = format_multi_profile_summary(&outcomes);
        let notification_type = aggregate_notification_type(&outcomes);
        *pending_notification.lock() = Some((notification_type, SharedString::from(summary)));
    });
}

/// Spawn a background thread that fetches one profile's devices and writes its outcome to the shared sheet state.
///
/// ### Arguments
/// - `profile`: Profile to fetch from.
/// - `state`: Shared sheet state to update.
/// - `sync_state`: Per-profile shared sync state.
/// - `http_agent`: Shared HTTP agent.
pub(super) fn spawn_profile_device_fetch(
    profile: ServerProfile,
    state: Arc<ShareSheetState>,
    sync_state: Arc<crate::fulgur::shared_state::SyncState>,
    http_agent: Arc<ureq::Agent>,
) {
    let needs_reconnect = !sync_state.connection_status.lock().is_connected();
    crate::fulgur::sync::synchronization::set_sync_server_connection_status(
        &sync_state.connection_status,
        SynchronizationStatus::Connecting,
    );
    *sync_state.connecting_since.lock() = Some(std::time::Instant::now());
    std::thread::spawn(move || {
        if !state.active.load(std::sync::atomic::Ordering::Acquire) {
            return;
        }
        if needs_reconnect {
            match crate::fulgur::sync::synchronization::initial_synchronization(
                &profile,
                &sync_state.token_state,
                &http_agent,
                &sync_state.pending_ack_share_ids,
            ) {
                Ok(begin_response) => {
                    *sync_state.device_name.lock() = Some(begin_response.device_name.clone());
                    *sync_state.pending_shared_files.lock() = begin_response.shares;
                    crate::fulgur::sync::synchronization::store_server_max_file_size(
                        &sync_state.max_file_size_bytes,
                        begin_response.max_file_size_bytes,
                    );
                    crate::fulgur::sync::synchronization::set_sync_server_connection_status(
                        &sync_state.connection_status,
                        SynchronizationStatus::Connected,
                    );
                }
                Err(e) => {
                    let status = SynchronizationStatus::from_error(&e);
                    crate::fulgur::sync::synchronization::set_sync_server_connection_status(
                        &sync_state.connection_status,
                        status,
                    );
                    *sync_state.connecting_since.lock() = None;
                    state.per_profile.write().insert(
                        profile.id.clone(),
                        ProfileFetchState::Failed(format!("{e}")),
                    );
                    return;
                }
            }
        }
        let result = get_devices(&profile, &sync_state.token_state, &http_agent);
        *sync_state.connecting_since.lock() = None;
        match result {
            Ok((devices, server_max_size)) => {
                crate::fulgur::sync::synchronization::store_server_max_file_size(
                    &sync_state.max_file_size_bytes,
                    server_max_size,
                );
                crate::fulgur::sync::synchronization::set_sync_server_connection_status(
                    &sync_state.connection_status,
                    SynchronizationStatus::Connected,
                );
                state.per_profile.write().insert(
                    profile.id.clone(),
                    ProfileFetchState::Loaded(Arc::new(devices)),
                );
                if needs_reconnect {
                    state.pending_sse_restarts.lock().push(profile.id.clone());
                }
            }
            Err(e) => {
                state.per_profile.write().insert(
                    profile.id.clone(),
                    ProfileFetchState::Failed(format!("{e}")),
                );
            }
        }
    });
}
