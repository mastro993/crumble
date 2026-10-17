DROP TABLE remote_audio_consents;
ALTER TABLE notes DROP COLUMN transcript_provider;
ALTER TABLE app_state DROP COLUMN transcription_locale;
ALTER TABLE app_state DROP COLUMN transcription_model;
ALTER TABLE app_state DROP COLUMN transcription_provider;
