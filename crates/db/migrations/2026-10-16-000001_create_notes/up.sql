CREATE TABLE notes (
    id TEXT PRIMARY KEY NOT NULL,       -- UUID v7, sorts by creation time
    title TEXT,                         -- NULL = no title
    memo TEXT NOT NULL DEFAULT '',
    -- The Event occurrence the Note is attached to (ADR 0004); all NULL for a Quick note.
    event_series_uid TEXT,              -- NULL also while the Event had not synced
    event_occurrence TEXT,              -- NULL for a non-repeating Event; '2026-10-13' or '2026-10-13T07:00:00Z'
    event_title TEXT,                   -- snapshot, to re-match when the key finds nothing
    event_start TEXT,                   -- snapshot, encoded like event_occurrence; NOT NULL = attached
    created_at TEXT NOT NULL,           -- RFC 3339 UTC, milliseconds
    updated_at TEXT NOT NULL,
    deleted_at TEXT,                    -- NULL = not deleted; soft delete, kept for sync
    CHECK ((event_start IS NULL) = (event_title IS NULL)),
    CHECK (event_start IS NOT NULL OR (event_series_uid IS NULL AND event_occurrence IS NULL))
);
-- An Event occurrence has at most one live Note. IFNULL, as a UNIQUE index tells NULLs apart and
-- a non-repeating Event's occurrence is NULL. A Note with no UID yet is kept single by Notes::create.
CREATE UNIQUE INDEX notes_event_key ON notes (event_series_uid, IFNULL(event_occurrence, ''))
    WHERE event_series_uid IS NOT NULL AND deleted_at IS NULL;
CREATE INDEX notes_event_snapshot ON notes (event_title, event_start);
CREATE INDEX notes_quick ON notes (created_at) WHERE event_start IS NULL AND deleted_at IS NULL;
