-- The Models Settings picks (ADR 0007). API keys are never here: they are in the Keychain.
ALTER TABLE app_state ADD COLUMN summary_provider TEXT   -- NULL = never picked: Apple's when available
    CHECK (summary_provider IN ('apple', 'claude', 'openai'));
ALTER TABLE app_state ADD COLUMN claude_model TEXT NOT NULL DEFAULT 'claude-sonnet-5-5';
ALTER TABLE app_state ADD COLUMN openai_base_url TEXT NOT NULL DEFAULT '';  -- '' = not set
ALTER TABLE app_state ADD COLUMN openai_model TEXT NOT NULL DEFAULT '';
