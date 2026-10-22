-- The Local Model that makes Transcripts while transcription_provider is 'local', by its slug
-- in Handy's model list (ADR 0010). NULL until one is picked.
ALTER TABLE app_state ADD COLUMN local_model TEXT;
