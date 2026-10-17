-- Whether Crumble suggests recording an Event as it starts, with a notification (ADR 0009).
ALTER TABLE app_state ADD COLUMN suggest_recording BOOLEAN NOT NULL DEFAULT TRUE;
