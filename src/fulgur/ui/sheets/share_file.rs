use crate::fulgur::{
    Fulgur,
    settings::{ProfileId, ServerProfile},
    sync::{
        share::{
            Device, ProfileShareOutcome, ShareFileRequest, format_multi_profile_summary,
            get_devices, get_icon, share_file,
        },
        synchronization::SynchronizationStatus,
    },
    ui::{
        icons::CustomIcon,
        notifications::progress::{CancelCallback, start_progress},
    },
};
use gpui::{
    App, Div, Element, Entity, FontWeight, SharedString, StatefulInteractiveElement,
    prelude::FluentBuilder,
};
use gpui::{Context, InteractiveElement, ParentElement, Styled, Window, div, px};
use gpui_component::{
    ActiveTheme, Icon, Sizable, WindowExt,
    button::{Button, ButtonVariants},
    h_flex,
    notification::NotificationType,
    scroll::ScrollableElement,
    spinner::Spinner,
    v_flex,
};
use parking_lot::{Mutex, RwLock};
use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
    time::Duration,
};

/// Per-profile fetch state shown in the share sheet.
pub enum ProfileFetchState {
    /// The device list is being fetched.
    Loading,
    /// Device list arrived from the server.
    Loaded(Arc<Vec<Device>>),
    /// The fetch failed; the message describes why.
    Failed(String),
}

/// Shared state owning everything the share sheet needs while it is open.
pub struct ShareSheetState {
    /// Active profiles when the sheet was opened, in declaration order.
    pub profiles: Vec<ServerProfile>,
    /// Per-profile fetch progress keyed by `ProfileId`.
    pub per_profile: Arc<RwLock<HashMap<ProfileId, ProfileFetchState>>>,
    /// User selection across all profiles, keyed by `(profile_id, device_id)`.
    pub selected: Arc<Mutex<Vec<(ProfileId, String)>>>,
    /// Profile ids whose SSE worker should be restarted because we had to
    /// reconnect during the device fetch. Drained by the render loop.
    pub pending_sse_restarts: Arc<Mutex<Vec<ProfileId>>>,
    /// Cleared to `false` while the sheet is open; flipped on Cancel/Share so
    /// background tasks can stop polling.
    pub active: Arc<AtomicBool>,
}

impl ShareSheetState {
    /// Build a fresh shared state with every profile starting in the `Loading` state.
    ///
    /// ### Arguments
    /// - `profiles`: Active profiles to render in the sheet.
    ///
    /// ### Returns
    /// - `Self`: An initialized state ready for fetch threads to populate.
    fn new(profiles: Vec<ServerProfile>) -> Self {
        let map: HashMap<ProfileId, ProfileFetchState> = profiles
            .iter()
            .map(|p| (p.id.clone(), ProfileFetchState::Loading))
            .collect();
        Self {
            profiles,
            per_profile: Arc::new(RwLock::new(map)),
            selected: Arc::new(Mutex::new(Vec::new())),
            pending_sse_restarts: Arc::new(Mutex::new(Vec::new())),
            active: Arc::new(AtomicBool::new(true)),
        }
    }

    /// Whether every profile has finished its fetch (Loaded or Failed).
    ///
    /// ### Returns
    /// - `true`: No profile remains in the `Loading` state.
    /// - `false`: At least one profile is still being fetched.
    fn all_settled(&self) -> bool {
        self.per_profile
            .read()
            .values()
            .all(|state| !matches!(state, ProfileFetchState::Loading))
    }
}

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

/// Create a single device row in the share sheet.
///
/// ### Arguments
/// - `profile_id`: The id of the profile that owns this device.
/// - `device`: The device to display.
/// - `is_selected`: Whether the device is currently selected.
/// - `selected_keys`: Shared mutable selection state, keyed by `(profile_id, device_id)`.
/// - `idx`: Stable index used to disambiguate the GPUI element id.
/// - `cx`: The application context.
///
/// ### Returns
/// - `impl Element`: The device row element.
fn make_device_item(
    profile_id: &ProfileId,
    device: &Device,
    is_selected: bool,
    selected_keys: Arc<Mutex<Vec<(ProfileId, String)>>>,
    idx: usize,
    cx: &App,
) -> impl Element {
    let profile_id = profile_id.clone();
    let device_id = device.id.clone();
    let device_name = device.name.clone();
    let device_expires = device.expires_at.clone();
    let device_for_icon = device;
    let has_public_key = device.public_key.is_some();
    h_flex()
        .id(("share-device-sheet", idx))
        .items_center()
        .justify_between()
        .w_full()
        .p_2()
        .my_2()
        .rounded_sm()
        .border_color(cx.theme().border)
        .border_1()
        .when(has_public_key, gpui::Styled::cursor_pointer)
        .when(!has_public_key, |this| this.opacity(0.5))
        .when(has_public_key, |this| {
            this.hover(|hover| hover.bg(cx.theme().muted))
        })
        .when(is_selected && has_public_key, |this| {
            this.bg(cx.theme().accent)
                .text_color(cx.theme().accent_foreground)
        })
        .child(
            v_flex()
                .gap_1()
                .child(
                    h_flex()
                        .items_center()
                        .justify_start()
                        .gap_2()
                        .child(get_icon(device_for_icon))
                        .child(div().child(device_name))
                        .child(div().text_xs().child(format!("Expires: {device_expires}"))),
                )
                .when(!has_public_key, |this| {
                    this.child(
                        div()
                            .text_xs()
                            .text_color(cx.theme().muted_foreground)
                            .child("No public key for this device"),
                    )
                }),
        )
        .when(is_selected && has_public_key, |this| {
            this.child(Icon::new(CustomIcon::Zap))
        })
        .when(has_public_key, |this| {
            this.on_click(move |_event, _window, _cx| {
                let mut keys = selected_keys.lock();
                let key = (profile_id.clone(), device_id.clone());
                if let Some(pos) = keys.iter().position(|existing| existing == &key) {
                    keys.remove(pos);
                } else {
                    keys.push(key);
                }
            })
        })
}

/// Build a profile section header used to delimit per-profile device rows.
///
/// ### Arguments
/// - `profile`: The profile to label.
/// - `cx`: The application context (used to read theme tokens).
///
/// ### Returns
/// - `Div`: The styled header element.
fn make_profile_header(profile: &ServerProfile, cx: &App) -> Div {
    let url_label = profile
        .server_url
        .clone()
        .unwrap_or_else(|| "(no URL)".to_string());
    v_flex()
        .gap_0p5()
        .pt_2()
        .child(
            div()
                .text_sm()
                .font_weight(FontWeight::SEMIBOLD)
                .text_color(cx.theme().foreground)
                .child(profile.name.clone()),
        )
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child(url_label),
        )
}

/// Render the placeholder shown while a profile's device list is loading.
///
/// ### Arguments
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The loading placeholder element.
fn make_loading_section(cx: &App) -> Div {
    h_flex()
        .gap_2()
        .items_center()
        .my_2()
        .child(Spinner::new().icon(CustomIcon::LoaderCircle).small())
        .child(
            div()
                .text_xs()
                .text_color(cx.theme().muted_foreground)
                .child("Loading devices..."),
        )
}

/// Render the failure message for a profile whose device fetch errored.
///
/// ### Arguments
/// - `message`: The error description from the worker thread.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The styled error placeholder.
fn make_error_section(message: &str, cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().danger)
        .my_2()
        .child(format!("Could not reach this profile: {message}"))
}

/// Render the empty-state placeholder when a profile returned zero devices.
///
/// ### Arguments
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The empty-state element.
fn make_empty_section(cx: &App) -> Div {
    div()
        .text_xs()
        .text_color(cx.theme().muted_foreground)
        .my_2()
        .child("No devices available.")
}

/// Build the grouped device list reflecting the current per-profile state.
///
/// Each profile (header + its devices, loading placeholder, or error message)
/// is wrapped in its own `v_flex` so the outer container can apply a wider
/// gap between profile groups than between rows inside a group.
///
/// ### Arguments
/// - `state`: Shared sheet state.
/// - `cx`: The application context.
///
/// ### Returns
/// - `Div`: The grouped list element.
fn make_device_list(state: &Arc<ShareSheetState>, cx: &App) -> Div {
    let mut container = v_flex().gap_6();
    let mut row_idx: usize = 0;
    let map = state.per_profile.read();
    for profile in &state.profiles {
        let mut group = div().child(make_profile_header(profile, cx));
        match map.get(&profile.id) {
            None | Some(ProfileFetchState::Loading) => {
                group = group.child(make_loading_section(cx));
            }
            Some(ProfileFetchState::Failed(message)) => {
                group = group.child(make_error_section(message, cx));
            }
            Some(ProfileFetchState::Loaded(devices)) => {
                if devices.is_empty() {
                    group = group.child(make_empty_section(cx));
                } else {
                    for device in devices.iter() {
                        let is_selected = state
                            .selected
                            .lock()
                            .iter()
                            .any(|(pid, did)| pid == &profile.id && did == &device.id);
                        group = group.child(make_device_item(
                            &profile.id,
                            device,
                            is_selected,
                            state.selected.clone(),
                            row_idx,
                            cx,
                        ));
                        row_idx += 1;
                    }
                }
            }
        }
        container = container.child(group);
    }
    container
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
fn handle_share_file(
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
fn spawn_profile_device_fetch(
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

/// Render the share sheet footer with Cancel and Share buttons.
///
/// ### Arguments
/// - `state`: Shared sheet state.
/// - `entity`: The Fulgur entity.
///
/// ### Returns
/// - `Div`: The footer element.
fn render_footer(state: Arc<ShareSheetState>, entity: Entity<Fulgur>) -> Div {
    let state_for_cancel = Arc::clone(&state);
    let entity_for_cancel = entity.clone();
    let state_for_share = state;
    let entity_for_share = entity;
    h_flex()
        .justify_end()
        .w_full()
        .gap_2()
        .child(
            Button::new("cancel-share")
                .child("Cancel")
                .small()
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    state_for_cancel
                        .active
                        .store(false, std::sync::atomic::Ordering::Release);
                    entity_for_cancel.update(cx, |this, _| {
                        this.share_sheet_state = None;
                    });
                    window.close_sheet(cx);
                }),
        )
        .child(
            Button::new("ok-share")
                .child("Share")
                .small()
                .primary()
                .cursor_pointer()
                .on_click(move |_, window, cx| {
                    handle_share_file(&state_for_share, &entity_for_share, window, cx);
                }),
        )
}

impl Fulgur {
    /// Return the list of profile ids whose devices should populate the share sheet.
    ///
    /// ### Returns
    /// - `Vec<ProfileId>`: Active only profile ids when the master sync switch is on, in declaration order.
    fn collect_active_profiles(&self) -> Vec<ServerProfile> {
        if !self
            .settings
            .app_settings
            .synchronization_settings
            .is_synchronization_activated
        {
            return Vec::new();
        }
        self.settings
            .app_settings
            .synchronization_settings
            .profiles
            .iter()
            .filter(|p| p.is_active)
            .cloned()
            .collect()
    }

    /// Open the share file sheet immediately and fetch each active profile's devices in the background.
    ///
    /// ### Arguments
    /// - `window`: The window context.
    /// - `cx`: The application context.
    pub fn open_share_file_sheet(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let active_profiles = self.collect_active_profiles();
        if active_profiles.is_empty() {
            log::warn!("Share aborted: no active profile or master sync is off");
            return;
        }
        if let Some(previous) = self.share_sheet_state.take() {
            previous
                .active
                .store(false, std::sync::atomic::Ordering::Release);
        }
        let state = Arc::new(ShareSheetState::new(active_profiles.clone()));
        self.share_sheet_state = Some(Arc::clone(&state));
        let shared = Fulgur::shared_state(cx);
        let http_agent = Arc::clone(&shared.http_agent);
        for profile in active_profiles {
            let sync_state = shared.sync_state_for(&profile.id);
            spawn_profile_device_fetch(
                profile,
                Arc::clone(&state),
                sync_state,
                Arc::clone(&http_agent),
            );
        }
        Self::start_share_sheet_render_pump(Arc::clone(&state), window, cx);
        Self::show_share_sheet(state, window, cx);
    }

    /// Drive periodic re-renders while the share sheet is open and at least one profile is still loading.
    ///
    /// ### Arguments
    /// - `state`: Shared sheet state.
    /// - `window`: The window context.
    /// - `cx`: The application context.
    fn start_share_sheet_render_pump(
        state: Arc<ShareSheetState>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let entity = cx.entity();
        window
            .spawn(cx, async move |async_cx| {
                loop {
                    async_cx
                        .background_executor()
                        .timer(Duration::from_millis(100))
                        .await;
                    if !state.active.load(std::sync::atomic::Ordering::Acquire) {
                        return;
                    }
                    let _ = async_cx.update(|_, cx| {
                        entity.update(cx, |_, cx| cx.notify());
                    });
                    if state.all_settled() {
                        return;
                    }
                }
            })
            .detach();
    }

    /// Drain pending SSE restart requests posted by share-sheet device fetches.
    ///
    /// ### Arguments
    /// - `_window`: The window context.
    /// - `cx`: The application context.
    pub fn process_pending_share_sheet(&mut self, _window: &mut Window, cx: &mut Context<Self>) {
        let Some(state) = self.share_sheet_state.as_ref().map(Arc::clone) else {
            return;
        };
        let to_restart: Vec<ProfileId> = std::mem::take(&mut *state.pending_sse_restarts.lock());
        for profile_id in to_restart {
            self.restart_sse_connection_for(&profile_id, cx);
        }
    }

    /// Open the share sheet UI for the given shared state. Each profile section reads live state on every render.
    ///
    /// ### Arguments
    /// - `state`: Shared sheet state.
    /// - `window`: The window context.
    /// - `cx`: The application context.
    fn show_share_sheet(state: Arc<ShareSheetState>, window: &mut Window, cx: &mut Context<Self>) {
        let entity = cx.entity();
        let viewport_height = window.viewport_size().height;
        window.open_sheet(cx, move |sheet, _window, cx| {
            #[cfg(target_os = "linux")]
            let sheet_overhead = px(200.0);
            #[cfg(not(target_os = "linux"))]
            let sheet_overhead = px(150.0);
            let max_height = px((viewport_height - sheet_overhead).into());
            let state_for_list = Arc::clone(&state);
            let state_for_footer = Arc::clone(&state);
            sheet
                .title("Share with...")
                .size(px(400.))
                .overlay(true)
                .child(
                    v_flex()
                        .overflow_y_scrollbar()
                        .gap_2()
                        .h(max_height)
                        .child(make_device_list(&state_for_list, cx)),
                )
                .footer(render_footer(state_for_footer, entity.clone()))
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use super::{Device, Fulgur, ProfileFetchState, ShareSheetState};
    use crate::fulgur::{
        settings::{ServerProfile, Settings},
        shared_state::SharedAppState,
        window_manager::WindowManager,
    };
    use gpui::{AppContext, Entity, TestAppContext, VisualTestContext, WindowOptions};
    use parking_lot::Mutex;
    use std::{cell::RefCell, path::PathBuf, sync::Arc};

    /// Initialize globals and open a test window with a Root-mounted `Fulgur`.
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

    fn make_device(id: &str) -> Device {
        Device {
            id: id.to_string(),
            name: format!("{id}-name"),
            device_type: "desktop".to_string(),
            public_key: Some("age1dummypublickey".to_string()),
            created_at: "2024-01-01T00:00:00Z".to_string(),
            expires_at: "2025-01-01T00:00:00Z".to_string(),
        }
    }

    #[test]
    fn test_share_sheet_state_starts_with_every_profile_loading() {
        let profile_a = ServerProfile::new("A");
        let profile_b = ServerProfile::new("B");
        let id_a = profile_a.id.clone();
        let id_b = profile_b.id.clone();
        let state = ShareSheetState::new(vec![profile_a, profile_b]);
        let map = state.per_profile.read();
        assert!(matches!(map.get(&id_a), Some(ProfileFetchState::Loading)));
        assert!(matches!(map.get(&id_b), Some(ProfileFetchState::Loading)));
        drop(map);
        assert!(!state.all_settled());
    }

    #[test]
    fn test_share_sheet_state_all_settled_when_loaded_or_failed() {
        let profile_a = ServerProfile::new("A");
        let profile_b = ServerProfile::new("B");
        let id_a = profile_a.id.clone();
        let id_b = profile_b.id.clone();
        let state = ShareSheetState::new(vec![profile_a, profile_b]);
        state.per_profile.write().insert(
            id_a,
            ProfileFetchState::Loaded(Arc::new(vec![make_device("d1")])),
        );
        state
            .per_profile
            .write()
            .insert(id_b, ProfileFetchState::Failed("nope".to_string()));
        assert!(state.all_settled());
    }

    #[test]
    fn test_share_sheet_state_not_settled_with_lingering_loading() {
        let profile_a = ServerProfile::new("A");
        let profile_b = ServerProfile::new("B");
        let id_a = profile_a.id.clone();
        let state = ShareSheetState::new(vec![profile_a, profile_b]);
        state
            .per_profile
            .write()
            .insert(id_a, ProfileFetchState::Loaded(Arc::new(vec![])));
        assert!(!state.all_settled());
    }

    #[gpui::test]
    fn test_process_pending_share_sheet_drains_sse_restart_queue(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                let profile = ServerProfile::new("queued");
                let id = profile.id.clone();
                this.settings
                    .app_settings
                    .synchronization_settings
                    .profiles
                    .push(profile.clone());
                let state = Arc::new(ShareSheetState::new(vec![profile]));
                state.pending_sse_restarts.lock().push(id.clone());
                this.share_sheet_state = Some(Arc::clone(&state));
                this.process_pending_share_sheet(window, cx);
                assert!(
                    state.pending_sse_restarts.lock().is_empty(),
                    "queue must be drained on each render"
                );
            });
        });
    }

    #[gpui::test]
    fn test_process_pending_share_sheet_no_state_is_a_noop(cx: &mut TestAppContext) {
        let (fulgur, mut visual_cx) = setup_fulgur(cx);
        visual_cx.update(|window, cx| {
            fulgur.update(cx, |this, cx| {
                this.share_sheet_state = None;
                this.process_pending_share_sheet(window, cx);
            });
        });
    }
}
