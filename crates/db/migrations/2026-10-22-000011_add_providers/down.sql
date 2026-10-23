-- Back to one Claude Model and one OpenAI-compatible server in app_state, under the old ids. What
-- has no old place is lost: other Providers' settings, and the Intelligence Provider.
ALTER TABLE app_state ADD COLUMN claude_model TEXT NOT NULL DEFAULT 'claude-sonnet-5-5';
ALTER TABLE app_state ADD COLUMN openai_base_url TEXT NOT NULL DEFAULT '';
ALTER TABLE app_state ADD COLUMN openai_model TEXT NOT NULL DEFAULT '';
ALTER TABLE app_state ADD COLUMN transcription_model TEXT NOT NULL DEFAULT 'whisper-1';
UPDATE app_state SET
    claude_model = COALESCE(
        (SELECT summary_model FROM providers WHERE id = 'anthropic'), claude_model),
    openai_base_url = COALESCE(
        (SELECT base_url FROM providers WHERE id = 'openai-compatible'), openai_base_url),
    openai_model = COALESCE(
        (SELECT summary_model FROM providers WHERE id = 'openai-compatible'), openai_model),
    transcription_model = COALESCE(
        (SELECT transcription_model FROM providers WHERE id = 'openai-compatible'),
        transcription_model);
ALTER TABLE app_state DROP COLUMN intelligence_provider;
DROP TABLE providers;

-- A choice of a Provider the old app lacks falls back to Apple's, as in 000010's down: OpenAI's
-- own API too, as the old 'openai' choice used the server's address and key.
ALTER TABLE app_state ADD COLUMN summary_provider_old TEXT
    CHECK (summary_provider_old IN ('apple', 'claude', 'openai'));
UPDATE app_state SET summary_provider_old = CASE summary_provider
    WHEN 'apple' THEN 'apple' WHEN 'anthropic' THEN 'claude' WHEN 'openai-compatible' THEN 'openai'
    END;
ALTER TABLE app_state DROP COLUMN summary_provider;
ALTER TABLE app_state RENAME COLUMN summary_provider_old TO summary_provider;

ALTER TABLE app_state ADD COLUMN transcription_provider_old TEXT NOT NULL DEFAULT 'apple'
    CHECK (transcription_provider_old IN ('apple', 'local', 'openai'));
UPDATE app_state SET transcription_provider_old = CASE transcription_provider
    WHEN 'local' THEN 'local' WHEN 'openai-compatible' THEN 'openai' ELSE 'apple' END;
ALTER TABLE app_state DROP COLUMN transcription_provider;
ALTER TABLE app_state RENAME COLUMN transcription_provider_old TO transcription_provider;

-- A Transcript or Summary only names who made it. One by OpenAI's own API reads as the server's,
-- as the old 'openai' did; another Provider's Transcript falls back to Apple's, which the old
-- CHECK allows, and its Summary keeps its id.
ALTER TABLE notes ADD COLUMN transcript_provider_old TEXT
    CHECK (transcript_provider_old IN ('apple', 'local', 'openai'));
UPDATE notes SET transcript_provider_old = CASE transcript_provider
    WHEN 'local' THEN 'local' WHEN 'openai' THEN 'openai' WHEN 'openai-compatible' THEN 'openai'
    ELSE 'apple' END
    WHERE transcript_provider IS NOT NULL;
ALTER TABLE notes DROP COLUMN transcript_provider;
ALTER TABLE notes RENAME COLUMN transcript_provider_old TO transcript_provider;

UPDATE notes SET summary_provider = CASE summary_provider
    WHEN 'anthropic' THEN 'claude' ELSE 'openai' END
    WHERE summary_provider IN ('anthropic', 'openai-compatible');

-- OpenAI's own consents already read as the server's at the same host; a duplicate is dropped.
UPDATE OR IGNORE remote_audio_consents SET provider = CASE provider
    WHEN 'anthropic' THEN 'claude' ELSE 'openai' END
    WHERE provider IN ('anthropic', 'openai-compatible');
DELETE FROM remote_audio_consents WHERE provider IN ('anthropic', 'openai-compatible');
