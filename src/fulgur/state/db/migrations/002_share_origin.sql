-- Origin of a tab opened from a received share, kept until the buffer is first
-- saved to a file. `share_size` is always set for such a tab, so its presence
-- marks the row as a received share.
ALTER TABLE tabs ADD COLUMN share_device_name TEXT;
-- RFC 3339 timestamp of when the sender shared the file.
ALTER TABLE tabs ADD COLUMN share_shared_at   TEXT;
-- Byte length of the decrypted content as received.
ALTER TABLE tabs ADD COLUMN share_size        INTEGER;
