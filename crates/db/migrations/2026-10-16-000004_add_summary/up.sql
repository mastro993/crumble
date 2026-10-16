-- A Note's Summary (ADR 0007), made from its Transcript only. A new Transcript, or none, clears
-- it in the same transaction, so a Summary never outlives the Transcript it was made from.
ALTER TABLE notes ADD COLUMN summary TEXT;           -- Markdown; NULL = no Summary
ALTER TABLE notes ADD COLUMN summary_provider TEXT   -- the Provider's id: 'apple', 'claude', 'openai'
    CHECK ((summary_provider IS NULL) = (summary IS NULL));
ALTER TABLE notes ADD COLUMN summary_model TEXT      -- e.g. 'claude-sonnet-5-5'
    CHECK ((summary_model IS NULL) = (summary IS NULL));
ALTER TABLE notes ADD COLUMN summarized_at TEXT      -- RFC 3339 UTC, milliseconds
    CHECK ((summarized_at IS NULL) = (summary IS NULL));
