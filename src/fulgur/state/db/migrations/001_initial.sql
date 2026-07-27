-- Initial session-state schema.
--
-- STRICT tables enforce the declared column types instead of applying SQLite's
-- default affinity coercion, which is what makes the TEXT/BLOB distinction below
-- meaningful.

CREATE TABLE windows (
    -- Persistent window identity, stable across restarts, so a save can update
    -- one window's row without touching any other window.
    id            INTEGER PRIMARY KEY,
    -- Restore order. Identity lives in `id`; this only decides which window is
    -- restored first.
    position      INTEGER NOT NULL,
    -- Active tab, referenced by tab identity rather than by list position.
    active_tab_id INTEGER,
    bounds_state  TEXT    NOT NULL,
    bounds_x      REAL    NOT NULL,
    bounds_y      REAL    NOT NULL,
    bounds_width  REAL    NOT NULL,
    bounds_height REAL    NOT NULL,
    display_id    INTEGER
) STRICT;

CREATE TABLE tabs (
    window_id    INTEGER NOT NULL REFERENCES windows(id) ON DELETE CASCADE,
    -- Tab identity, unique within its window only, hence the composite key.
    id           INTEGER NOT NULL,
    position     INTEGER NOT NULL,
    title        TEXT    NOT NULL,
    -- BLOB because OS paths are not guaranteed to be valid UTF-8.
    file_path    BLOB,
    -- Unsaved buffer text. NULL when the tab is clean, when it exceeds the
    -- large-file threshold, or when persisting unsaved buffers is disabled.
    content      TEXT,
    -- FNV-1a fingerprint of `content` and its byte length, used to decide
    -- whether a save has to rewrite the text at all. Byte length is stored
    -- explicitly because length() counts characters on a TEXT column.
    content_hash INTEGER,
    content_len  INTEGER,
    last_saved   TEXT,
    log_view     INTEGER NOT NULL,
    color_tag    TEXT,
    -- Remote tabs never persist credentials, only their location.
    remote_host  TEXT,
    remote_port  INTEGER,
    remote_user  TEXT,
    remote_path  TEXT,
    PRIMARY KEY (window_id, id)
) STRICT;

CREATE INDEX tabs_by_window ON tabs(window_id, position);
