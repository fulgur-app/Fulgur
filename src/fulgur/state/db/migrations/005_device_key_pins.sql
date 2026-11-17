-- Public keys of share recipients, pinned the first time a server reported them
-- (trust on first use), so a server swapping a device key is detected.
CREATE TABLE device_key_pins (
    -- Sync profile the device belongs to; device ids are only unique per server.
    profile_id TEXT NOT NULL,
    device_id  TEXT NOT NULL,
    -- The age X25519 recipient (`age1...`) trusted for this device.
    public_key TEXT NOT NULL,
    PRIMARY KEY (profile_id, device_id)
) STRICT;
