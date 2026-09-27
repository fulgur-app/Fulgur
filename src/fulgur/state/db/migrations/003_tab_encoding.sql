-- Encoding label the buffer was decoded from, so a restored buffer is saved
-- back in its original encoding. NULL for rows written before this column
-- existed, which restore as UTF-8.
ALTER TABLE tabs ADD COLUMN encoding     TEXT;
-- Whether decoding the file replaced undecodable bytes with U+FFFD, so the
-- lossy-save confirmation still guards a restored buffer.
ALTER TABLE tabs ADD COLUMN lossy_decode INTEGER NOT NULL DEFAULT 0;
