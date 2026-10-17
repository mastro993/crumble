-- A Note's Transcript (ADR 0006). A Note has at most one, replaced wholesale: its segments are
-- deleted for good and written again in one transaction, and the Note's updated_at marks it.
ALTER TABLE notes ADD COLUMN transcript_model TEXT;  -- e.g. 'Apple SpeechAnalyzer'; NULL = no Transcript
ALTER TABLE notes ADD COLUMN transcript_locale TEXT  -- the locale it was made in, e.g. 'en_US'
    CHECK ((transcript_locale IS NULL) = (transcript_model IS NULL));
ALTER TABLE notes ADD COLUMN transcribed_at TEXT     -- RFC 3339 UTC, milliseconds
    CHECK ((transcribed_at IS NULL) = (transcript_model IS NULL));
CREATE TABLE transcript_segments (
    id TEXT PRIMARY KEY NOT NULL,                     -- UUID v7
    note_id TEXT NOT NULL REFERENCES notes (id),
    track TEXT NOT NULL CHECK (track IN ('mic', 'system')), -- who spoke: the user, or the call
    start_ms INTEGER NOT NULL,                        -- from the Recording's start
    end_ms INTEGER NOT NULL,
    text TEXT NOT NULL
);
CREATE INDEX transcript_segments_note ON transcript_segments (note_id, start_ms);
