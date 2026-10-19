-- A Transcript or choice made by a Local Model has no Provider to fall back to but Apple's.
ALTER TABLE app_state ADD COLUMN transcription_provider_old TEXT NOT NULL DEFAULT 'apple'
    CHECK (transcription_provider_old IN ('apple', 'openai'));
UPDATE app_state SET transcription_provider_old = transcription_provider
    WHERE transcription_provider != 'local';
ALTER TABLE app_state DROP COLUMN transcription_provider;
ALTER TABLE app_state RENAME COLUMN transcription_provider_old TO transcription_provider;

ALTER TABLE notes ADD COLUMN transcript_provider_old TEXT
    CHECK (transcript_provider_old IN ('apple', 'openai'));
UPDATE notes SET transcript_provider_old =
    CASE transcript_provider WHEN 'local' THEN 'apple' ELSE transcript_provider END;
ALTER TABLE notes DROP COLUMN transcript_provider;
ALTER TABLE notes RENAME COLUMN transcript_provider_old TO transcript_provider;
