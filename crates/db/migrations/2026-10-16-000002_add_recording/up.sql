-- A Note's Recording (ADR 0005). The audio lives in recordings/<note id>/ beside the database.
ALTER TABLE notes ADD COLUMN recording_started_at TEXT;    -- RFC 3339 UTC, milliseconds; NULL = no Recording
ALTER TABLE notes ADD COLUMN recording_duration_ms INTEGER -- NULL while recording, or after a crash until recovered
    CHECK (recording_duration_ms IS NULL OR recording_started_at IS NOT NULL);
-- The Agenda lists a Quick note at its Recording's start, else when it was made.
DROP INDEX notes_quick;
CREATE INDEX notes_quick ON notes (COALESCE(recording_started_at, created_at))
    WHERE event_start IS NULL AND deleted_at IS NULL;
