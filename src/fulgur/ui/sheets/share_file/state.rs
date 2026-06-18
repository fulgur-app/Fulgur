use crate::fulgur::{
    settings::{ProfileId, ServerProfile},
    sync::share::Device,
};
use parking_lot::{Mutex, RwLock};
use std::{
    collections::HashMap,
    sync::{Arc, atomic::AtomicBool},
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
    pub(super) fn new(profiles: Vec<ServerProfile>) -> Self {
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
    pub(super) fn all_settled(&self) -> bool {
        self.per_profile
            .read()
            .values()
            .all(|state| !matches!(state, ProfileFetchState::Loading))
    }
}
