DROP INDEX notes_quick;
CREATE INDEX notes_quick ON notes (created_at) WHERE event_start IS NULL AND deleted_at IS NULL;
ALTER TABLE notes DROP COLUMN recording_duration_ms;
ALTER TABLE notes DROP COLUMN recording_started_at;
