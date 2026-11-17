use std::collections::{BTreeMap, btree_map::Entry};

/// How a public key reported by a server compares with the key pinned for that device.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceKeyStatus {
    /// No key was ever pinned for the device: it is seen for the first time.
    Unpinned,
    /// The reported key is the pinned one.
    Trusted,
    /// The reported key differs from the pinned one, which is carried here.
    Changed { pinned_key: String },
}

/// Share recipient public keys trusted on first use, per sync profile and device.
///
/// A device id is only unique within one server, so pins are keyed on the
/// `(profile_id, device_id)` pair. Keys are the age recipients (`age1...`)
/// shares are encrypted to.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DeviceKeyPins {
    pins: BTreeMap<(String, String), String>,
}

impl DeviceKeyPins {
    /// Build the pins from `(profile_id, device_id, public_key)` entries.
    ///
    /// ### Arguments
    /// - `entries`: The pinned keys; a later entry for the same device wins
    ///
    /// ### Returns
    /// - `DeviceKeyPins`: The pins holding every entry
    #[must_use]
    pub fn from_entries(entries: impl IntoIterator<Item = (String, String, String)>) -> Self {
        let pins = entries
            .into_iter()
            .map(|(profile_id, device_id, public_key)| ((profile_id, device_id), public_key))
            .collect();
        Self { pins }
    }

    /// The pinned keys, ordered by profile then device.
    ///
    /// ### Returns
    /// - `impl Iterator<Item = (&str, &str, &str)>`: The `(profile_id, device_id, public_key)` entries
    pub fn entries(&self) -> impl Iterator<Item = (&str, &str, &str)> {
        self.pins
            .iter()
            .map(|((profile_id, device_id), public_key)| {
                (profile_id.as_str(), device_id.as_str(), public_key.as_str())
            })
    }

    /// Compare a server-reported key with the key pinned for the device.
    ///
    /// ### Arguments
    /// - `profile_id`: The sync profile the device was listed by
    /// - `device_id`: The device id
    /// - `public_key`: The key the server reported for the device
    ///
    /// ### Returns
    /// - `DeviceKeyStatus`: Whether the key is unpinned, trusted, or changed
    #[must_use]
    pub fn status(&self, profile_id: &str, device_id: &str, public_key: &str) -> DeviceKeyStatus {
        match self.pinned_key(profile_id, device_id) {
            None => DeviceKeyStatus::Unpinned,
            Some(pinned) if pinned == public_key => DeviceKeyStatus::Trusted,
            Some(pinned) => DeviceKeyStatus::Changed {
                pinned_key: pinned.to_string(),
            },
        }
    }

    /// Pin the keys of devices seen for the first time, leaving known devices alone.
    ///
    /// ### Arguments
    /// - `profile_id`: The sync profile the devices were listed by
    /// - `devices`: The `(device_id, public_key)` pairs the server reported
    ///
    /// ### Returns
    /// - `bool`: `true` when at least one key was pinned
    pub fn pin_first_seen<'a>(
        &mut self,
        profile_id: &str,
        devices: impl IntoIterator<Item = (&'a str, &'a str)>,
    ) -> bool {
        let mut pinned_any = false;
        for (device_id, public_key) in devices {
            if let Entry::Vacant(slot) = self
                .pins
                .entry((profile_id.to_string(), device_id.to_string()))
            {
                slot.insert(public_key.to_string());
                pinned_any = true;
            }
        }
        pinned_any
    }

    /// Trust a key for a device, replacing any key pinned before.
    ///
    /// ### Arguments
    /// - `profile_id`: The sync profile the device belongs to
    /// - `device_id`: The device id
    /// - `public_key`: The key to trust
    ///
    /// ### Returns
    /// - `bool`: `true` when the pin changed, `false` when the key was already trusted
    pub fn trust(&mut self, profile_id: &str, device_id: &str, public_key: &str) -> bool {
        if self.pinned_key(profile_id, device_id) == Some(public_key) {
            return false;
        }
        self.pins.insert(
            (profile_id.to_string(), device_id.to_string()),
            public_key.to_string(),
        );
        true
    }

    /// Forget every pin of a sync profile.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose pins are dropped
    ///
    /// ### Returns
    /// - `bool`: `true` when at least one pin was dropped
    pub fn forget_profile(&mut self, profile_id: &str) -> bool {
        let pinned = self.pins.len();
        self.pins.retain(|(owner, _), _| owner != profile_id);
        self.pins.len() != pinned
    }

    /// The key pinned for a device.
    ///
    /// ### Arguments
    /// - `profile_id`: The sync profile the device belongs to
    /// - `device_id`: The device id
    ///
    /// ### Returns
    /// - `Some(&str)`: The pinned key
    /// - `None`: No key is pinned for the device
    fn pinned_key(&self, profile_id: &str, device_id: &str) -> Option<&str> {
        self.pins
            .get(&(profile_id.to_string(), device_id.to_string()))
            .map(String::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::{DeviceKeyPins, DeviceKeyStatus};

    #[test]
    fn an_unknown_device_is_unpinned() {
        let pins = DeviceKeyPins::default();
        assert_eq!(pins.status("p", "d", "age1a"), DeviceKeyStatus::Unpinned);
    }

    #[test]
    fn pin_first_seen_trusts_new_devices_only() {
        let mut pins = DeviceKeyPins::default();
        assert!(pins.pin_first_seen("p", [("d1", "age1a")]));
        assert!(
            !pins.pin_first_seen("p", [("d1", "age1swapped")]),
            "a known device must keep its pin"
        );
        assert_eq!(pins.status("p", "d1", "age1a"), DeviceKeyStatus::Trusted);
        assert_eq!(
            pins.status("p", "d1", "age1swapped"),
            DeviceKeyStatus::Changed {
                pinned_key: "age1a".to_string()
            }
        );
    }

    #[test]
    fn the_same_device_id_is_pinned_separately_per_profile() {
        let mut pins = DeviceKeyPins::default();
        pins.pin_first_seen("home", [("d", "age1home")]);
        assert_eq!(
            pins.status("work", "d", "age1work"),
            DeviceKeyStatus::Unpinned
        );
        pins.pin_first_seen("work", [("d", "age1work")]);
        assert_eq!(
            pins.status("home", "d", "age1home"),
            DeviceKeyStatus::Trusted
        );
        assert_eq!(
            pins.status("work", "d", "age1work"),
            DeviceKeyStatus::Trusted
        );
    }

    #[test]
    fn trust_replaces_a_changed_key() {
        let mut pins = DeviceKeyPins::from_entries([(
            "p".to_string(),
            "d".to_string(),
            "age1old".to_string(),
        )]);
        assert!(pins.trust("p", "d", "age1new"));
        assert_eq!(pins.status("p", "d", "age1new"), DeviceKeyStatus::Trusted);
        assert!(!pins.trust("p", "d", "age1new"), "already trusted");
    }

    #[test]
    fn forget_profile_drops_only_that_profile() {
        let mut pins = DeviceKeyPins::default();
        pins.pin_first_seen("home", [("d1", "age1a"), ("d2", "age1b")]);
        pins.pin_first_seen("work", [("d1", "age1c")]);
        assert!(pins.forget_profile("home"));
        assert!(!pins.forget_profile("home"));
        assert_eq!(
            pins.entries().collect::<Vec<_>>(),
            [("work", "d1", "age1c")]
        );
    }
}
