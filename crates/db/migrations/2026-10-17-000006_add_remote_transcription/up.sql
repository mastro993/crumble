-- Transcription on a remote Model (ADR 0008). The server is the OpenAI-compatible one Summaries
-- use (openai_base_url, and its key in the Keychain); only the Model differs.
ALTER TABLE app_state ADD COLUMN transcription_provider TEXT NOT NULL DEFAULT 'apple'
    CHECK (transcription_provider IN ('apple', 'openai'));
ALTER TABLE app_state ADD COLUMN transcription_model TEXT NOT NULL DEFAULT 'whisper-1'; -- on the server
ALTER TABLE app_state ADD COLUMN transcription_locale TEXT;  -- e.g. 'it_IT'; NULL = the Mac's locale

-- Who made a Transcript. Set whenever transcript_model is; SQLite checks a new column's CHECK
-- against existing rows, so the pairing is kept by crumble-notes rather than a constraint.
ALTER TABLE notes ADD COLUMN transcript_provider TEXT
    CHECK (transcript_provider IN ('apple', 'openai'));
UPDATE notes SET transcript_provider = 'apple' WHERE transcript_model IS NOT NULL;

-- Each Model the user agreed to send Recordings' audio to, once (ADR 0008). The host is part of
-- the key: the same Model on another server is another party.
CREATE TABLE remote_audio_consents (
    provider TEXT NOT NULL,         -- the Provider's id, e.g. 'openai'
    model TEXT NOT NULL,            -- e.g. 'whisper-1'
    host TEXT NOT NULL,             -- lowercase, without port, e.g. 'api.openai.com'
    acknowledged_at TEXT NOT NULL,  -- RFC 3339 UTC, milliseconds
    PRIMARY KEY (provider, model, host)
);
