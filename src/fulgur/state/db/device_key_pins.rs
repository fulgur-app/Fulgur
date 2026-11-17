//! Persisting the share recipient public keys trusted on first use.

use super::StateDb;
use crate::fulgur::state::persistence::DeviceKeyPins;
use anyhow::anyhow;
use rusqlite::{TransactionBehavior, params};
use std::collections::HashMap;

impl StateDb {
    /// Read the pinned device public keys.
    ///
    /// ### Errors
    /// - Returns an error if the query fails or if a row cannot be decoded.
    ///
    /// ### Returns
    /// - `Ok(DeviceKeyPins)`: The persisted pins
    /// - `Err(anyhow::Error)`: The pins could not be read
    pub fn load_device_key_pins(&self) -> anyhow::Result<DeviceKeyPins> {
        let mut stmt = self
            .conn
            .prepare("SELECT profile_id, device_id, public_key FROM device_key_pins")
            .map_err(|e| anyhow!("Failed to prepare the device key pins query: {e}"))?;
        let entries = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(|e| anyhow!("Failed to query the device key pins: {e}"))?
            .collect::<Result<Vec<_>, _>>()
            .map_err(|e| anyhow!("Failed to decode a device key pin: {e}"))?;
        Ok(DeviceKeyPins::from_entries(entries))
    }

    /// Persist the pinned device public keys, writing only the rows that differ.
    ///
    /// ### Arguments
    /// - `pins`: The pins to persist
    ///
    /// ### Errors
    /// - Returns an error if the transaction cannot be opened, if a statement
    ///   fails, or if the commit fails.
    ///
    /// ### Returns
    /// - `Ok(usize)`: Number of rows inserted, updated or deleted
    /// - `Err(anyhow::Error)`: The pins could not be persisted
    pub fn apply_device_key_pins(&mut self, pins: &DeviceKeyPins) -> anyhow::Result<usize> {
        let tx = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)
            .map_err(|e| anyhow!("Failed to begin the device key pins transaction: {e}"))?;
        let mut stored: HashMap<(String, String), String> = {
            let mut stmt = tx
                .prepare("SELECT profile_id, device_id, public_key FROM device_key_pins")
                .map_err(|e| anyhow!("Failed to prepare the stored device key pins query: {e}"))?;
            stmt.query_map([], |row| Ok(((row.get(0)?, row.get(1)?), row.get(2)?)))
                .map_err(|e| anyhow!("Failed to query the stored device key pins: {e}"))?
                .collect::<Result<_, _>>()
                .map_err(|e| anyhow!("Failed to decode a stored device key pin: {e}"))?
        };
        let mut rows_written = 0;
        for (profile_id, device_id, public_key) in pins.entries() {
            let slot = (profile_id.to_string(), device_id.to_string());
            if stored.remove(&slot).as_deref() == Some(public_key) {
                continue;
            }
            tx.execute(
                "INSERT INTO device_key_pins (profile_id, device_id, public_key) VALUES (?1, ?2, ?3)
                 ON CONFLICT(profile_id, device_id) DO UPDATE SET public_key = excluded.public_key",
                params![profile_id, device_id, public_key],
            )
            .map_err(|e| anyhow!("Failed to write device key pin: {e}"))?;
            rows_written += 1;
        }
        for (profile_id, device_id) in stored.keys() {
            rows_written += tx
                .execute(
                    "DELETE FROM device_key_pins WHERE profile_id = ?1 AND device_id = ?2",
                    params![profile_id, device_id],
                )
                .map_err(|e| anyhow!("Failed to delete device key pin: {e}"))?;
        }
        tx.commit()
            .map_err(|e| anyhow!("Failed to commit the device key pins transaction: {e}"))?;
        Ok(rows_written)
    }
}
