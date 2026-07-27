//! One-time import of the pre-`SQLite` `state.json` document.
//!
//! To be removed in v1.2

use super::StateDb;
use crate::fulgur::state::persistence::{WindowState, WindowsState};
use anyhow::anyhow;
use std::fs;
use std::path::Path;

/// File name of the legacy state document inside the config directory.
pub const LEGACY_STATE_FILE_NAME: &str = "state.json";

/// Import a legacy `state.json` into a freshly created database.
///
/// ### Arguments
/// - `json_path`: Path to the legacy state document
/// - `db`: The database to populate
///
/// ### Errors
/// - Returns an error if the document cannot be read or parsed, or if the rows cannot be written.
///
/// ### Returns
/// - `Ok(usize)`: How many windows were imported
/// - `Err(anyhow::Error)`: The document could not be imported
pub fn import_legacy_json(json_path: &Path, db: &mut StateDb) -> anyhow::Result<usize> {
    let mut state = read_legacy_json(json_path)?;
    assign_missing_identity(&mut state);
    let window_count = state.windows.len();
    db.apply(&state)?;
    log::info!(
        "Imported {window_count} window(s) from the legacy state document '{}'",
        json_path.display()
    );
    Ok(window_count)
}

/// Parse a legacy state document, falling back to its `.bak` companion.
///
/// ### Arguments
/// - `path`: Path to the legacy state document
///
/// ### Errors
/// - Returns an error if neither the document nor its backup can be read and parsed.
///
/// ### Returns
/// - `Ok(WindowsState)`: The parsed session
/// - `Err(anyhow::Error)`: Both the document and its backup are unusable
fn read_legacy_json(path: &Path) -> anyhow::Result<WindowsState> {
    let json =
        fs::read_to_string(path).map_err(|e| anyhow!("Failed to read legacy state file: {e}"))?;
    match serde_json::from_str::<WindowsState>(&json) {
        Ok(state) => Ok(state),
        Err(primary_err) => {
            let backup = crate::fulgur::utils::atomic_write::backup_path_for(path);
            log::warn!(
                "Legacy state file is corrupted ({primary_err}), attempting recovery from '{}'",
                backup.display()
            );
            let bak_json = fs::read_to_string(&backup)
                .map_err(|_| anyhow!("Failed to parse legacy state: {primary_err}"))?;
            let state = serde_json::from_str::<WindowsState>(&bak_json).map_err(|bak_err| {
                anyhow!(
                    "Legacy state and backup are both corrupted: primary={primary_err}, backup={bak_err}"
                )
            })?;
            log::warn!("Legacy state recovered from backup '{}'", backup.display());
            Ok(state)
        }
    }
}

/// Give imported windows and tabs the identity the legacy format lacked.
///
/// ### Arguments
/// - `state`: The freshly parsed session, mutated in place
fn assign_missing_identity(state: &mut WindowsState) {
    for window in &mut state.windows {
        window.window_id = WindowState::allocate_id();
        for (index, tab) in window.tabs.iter_mut().enumerate() {
            tab.tab_id = u64::try_from(index).unwrap_or_default();
        }
    }
}
