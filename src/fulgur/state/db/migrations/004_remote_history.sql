-- URLs recently opened through the Open Remote File dialog, in canonical form
-- and never carrying a password.
CREATE TABLE remote_history (
    -- The URL is the row identity, so moving an entry to the top only rewrites
    -- the positions that changed.
    url      TEXT    PRIMARY KEY,
    -- Recency rank, 0 being the most recently used URL.
    position INTEGER NOT NULL
) STRICT;
