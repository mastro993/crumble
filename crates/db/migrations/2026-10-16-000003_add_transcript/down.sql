DROP INDEX transcript_segments_note;
DROP TABLE transcript_segments;
ALTER TABLE notes DROP COLUMN transcribed_at;
ALTER TABLE notes DROP COLUMN transcript_locale;
ALTER TABLE notes DROP COLUMN transcript_model;
