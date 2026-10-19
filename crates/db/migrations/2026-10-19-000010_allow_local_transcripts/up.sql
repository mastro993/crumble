-- Local Models make Transcripts too (ADR 0010), as Provider 'local'. SQLite cannot change a
-- CHECK, so each column is made again with the wider one: added under a new name, copied, then
-- renamed into place.
ALTER TABLE app_state ADD COLUMN transcription_provider_new TEXT NOT NULL DEFAULT 'apple'
    CHECK (transcription_provider_new IN ('apple', 'local', 'openai'));
UPDATE app_state SET transcription_provider_new = transcription_provider;
ALTER TABLE app_state DROP COLUMN transcription_provider;
ALTER TABLE app_state RENAME COLUMN transcription_provider_new TO transcription_provider;

ALTER TABLE notes ADD COLUMN transcript_provider_new TEXT
    CHECK (transcript_provider_new IN ('apple', 'local', 'openai'));
UPDATE notes SET transcript_provider_new = transcript_provider;
ALTER TABLE notes DROP COLUMN transcript_provider;
ALTER TABLE notes RENAME COLUMN transcript_provider_new TO transcript_provider;
