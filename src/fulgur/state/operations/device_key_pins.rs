use crate::fulgur::{Fulgur, shared_state::SharedAppState, state::DeviceKeyPins};
use gpui_kit::{App, BorrowAppContext};

impl Fulgur {
    /// Pin the public keys of devices a server reports for the first time, and persist them.
    ///
    /// ### Arguments
    /// - `profile_id`: The sync profile that listed the devices
    /// - `devices`: The `(device_id, public_key)` pairs the server reported
    /// - `cx`: The application context
    pub fn pin_first_seen_device_keys<'a>(
        profile_id: &str,
        devices: impl IntoIterator<Item = (&'a str, &'a str)>,
        cx: &mut App,
    ) {
        let mut pins = Self::shared_state(cx).device_key_pins.clone();
        if pins.pin_first_seen(profile_id, devices) {
            Self::store_device_key_pins(pins, cx);
        }
    }

    /// Trust a device's current public key, replacing the one pinned before, and persist it.
    ///
    /// ### Arguments
    /// - `profile_id`: The sync profile the device belongs to
    /// - `device_id`: The device id
    /// - `public_key`: The key the user confirmed for the device
    /// - `cx`: The application context
    pub fn trust_device_key(profile_id: &str, device_id: &str, public_key: &str, cx: &mut App) {
        let mut pins = Self::shared_state(cx).device_key_pins.clone();
        if pins.trust(profile_id, device_id, public_key) {
            log::info!("Trusted a new public key for device '{device_id}'");
            Self::store_device_key_pins(pins, cx);
        }
    }

    /// Forget every device key pinned through a sync profile, and persist the change.
    ///
    /// ### Arguments
    /// - `profile_id`: The profile whose pins are dropped
    /// - `cx`: The application context
    pub fn forget_profile_device_keys(profile_id: &str, cx: &mut App) {
        let mut pins = Self::shared_state(cx).device_key_pins.clone();
        if pins.forget_profile(profile_id) {
            Self::store_device_key_pins(pins, cx);
        }
    }

    /// Publish changed device key pins to every window and hand them to the state writer.
    ///
    /// ### Arguments
    /// - `pins`: The new pins
    /// - `cx`: The application context
    fn store_device_key_pins(pins: DeviceKeyPins, cx: &mut App) {
        cx.update_global::<SharedAppState, _>(|shared, _| {
            shared.state_writer.save_device_key_pins(pins.clone());
            shared.device_key_pins = pins;
        });
    }
}

#[cfg(all(test, feature = "gpui-test-support"))]
mod tests {
    use crate::fulgur::{Fulgur, state::DeviceKeyStatus};
    use crate::test_support::setup_test_globals;
    use gpui_kit::TestAppContext;

    /// Read the in-memory status of a device key.
    fn status(cx: &mut TestAppContext, device_id: &str, key: &str) -> DeviceKeyStatus {
        cx.update(|cx| {
            Fulgur::shared_state(cx)
                .device_key_pins
                .status("profile", device_id, key)
        })
    }

    #[gpui_kit::test]
    fn a_swapped_key_is_reported_until_the_user_trusts_it(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            Fulgur::pin_first_seen_device_keys("profile", [("laptop", "age1original")], cx);
            Fulgur::pin_first_seen_device_keys("profile", [("laptop", "age1swapped")], cx);
        });
        assert_eq!(
            status(cx, "laptop", "age1swapped"),
            DeviceKeyStatus::Changed {
                pinned_key: "age1original".to_string()
            }
        );
        cx.update(|cx| Fulgur::trust_device_key("profile", "laptop", "age1swapped", cx));
        assert_eq!(
            status(cx, "laptop", "age1swapped"),
            DeviceKeyStatus::Trusted
        );
    }

    #[gpui_kit::test]
    fn forgetting_a_profile_unpins_its_devices(cx: &mut TestAppContext) {
        setup_test_globals(cx);
        cx.update(|cx| {
            Fulgur::pin_first_seen_device_keys("profile", [("laptop", "age1original")], cx);
            Fulgur::forget_profile_device_keys("profile", cx);
        });
        assert_eq!(
            status(cx, "laptop", "age1original"),
            DeviceKeyStatus::Unpinned
        );
    }
}
