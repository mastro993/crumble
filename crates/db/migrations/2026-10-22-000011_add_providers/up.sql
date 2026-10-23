-- Settings' remote Providers (ADR 0011): one row per remote Provider the user changed. NULL is
-- the Provider's own default, as crumble-remote's registry gives it. API keys are never here:
-- they are in the Keychain.
CREATE TABLE providers (
    id TEXT PRIMARY KEY NOT NULL,   -- the Provider's id, e.g. 'anthropic'
    base_url TEXT,                  -- NULL = its usual address
    summary_model TEXT,             -- NULL = its default Model for each job
    transcription_model TEXT,
    intelligence_model TEXT
);

-- Claude's Model and the OpenAI-compatible server's settings move into it, only when there is
-- something besides the defaults to keep: '' was "not set", and 'claude-sonnet-5-5' the Model
-- Claude started with. A server's Transcription Model is kept as it was, 'whisper-1' included.
INSERT INTO providers (id, summary_model)
    SELECT 'anthropic', NULLIF(NULLIF(claude_model, ''), 'claude-sonnet-5-5') FROM app_state
    WHERE NULLIF(NULLIF(claude_model, ''), 'claude-sonnet-5-5') IS NOT NULL;
INSERT INTO providers (id, base_url, summary_model, transcription_model)
    SELECT 'openai-compatible', NULLIF(openai_base_url, ''), NULLIF(openai_model, ''),
        NULLIF(transcription_model, '')
    FROM app_state
    WHERE openai_base_url != '' OR openai_model != '' OR transcription_model NOT IN ('', 'whisper-1');

-- Provider ids now come from crumble-remote's registry: Claude is 'anthropic', and 'openai', the
-- OpenAI-compatible server until now, is 'openai-compatible', as 'openai' now names OpenAI's own
-- API. The columns that listed the ids in a CHECK are made again without one, so a new Provider
-- needs no migration: added under a new name, copied with the new ids, then renamed into place.
ALTER TABLE app_state ADD COLUMN summary_provider_new TEXT;  -- NULL = never picked: Apple's when available
UPDATE app_state SET summary_provider_new = CASE summary_provider
    WHEN 'claude' THEN 'anthropic' WHEN 'openai' THEN 'openai-compatible' ELSE summary_provider END;
ALTER TABLE app_state DROP COLUMN summary_provider;
ALTER TABLE app_state RENAME COLUMN summary_provider_new TO summary_provider;

ALTER TABLE app_state ADD COLUMN transcription_provider_new TEXT NOT NULL DEFAULT 'apple';
UPDATE app_state SET transcription_provider_new = CASE transcription_provider
    WHEN 'openai' THEN 'openai-compatible' ELSE transcription_provider END;
ALTER TABLE app_state DROP COLUMN transcription_provider;
ALTER TABLE app_state RENAME COLUMN transcription_provider_new TO transcription_provider;

ALTER TABLE notes ADD COLUMN transcript_provider_new TEXT;
UPDATE notes SET transcript_provider_new = CASE transcript_provider
    WHEN 'openai' THEN 'openai-compatible' ELSE transcript_provider END;
ALTER TABLE notes DROP COLUMN transcript_provider;
ALTER TABLE notes RENAME COLUMN transcript_provider_new TO transcript_provider;

UPDATE notes SET summary_provider = CASE summary_provider
    WHEN 'claude' THEN 'anthropic' ELSE 'openai-compatible' END
    WHERE summary_provider IN ('claude', 'openai');

-- A consent keeps its Model and host: the same party gets the audio, so it stays valid.
UPDATE remote_audio_consents SET provider = CASE provider
    WHEN 'claude' THEN 'anthropic' ELSE 'openai-compatible' END
    WHERE provider IN ('claude', 'openai');

-- The Provider picked for Intelligence, Crumble's chat; NULL until one is.
ALTER TABLE app_state ADD COLUMN intelligence_provider TEXT;

ALTER TABLE app_state DROP COLUMN claude_model;
ALTER TABLE app_state DROP COLUMN openai_base_url;
ALTER TABLE app_state DROP COLUMN openai_model;
ALTER TABLE app_state DROP COLUMN transcription_model;
