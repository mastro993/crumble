CREATE TABLE app_state (
    id INTEGER PRIMARY KEY NOT NULL CHECK (id = 1),
    first_day_of_week INTEGER NOT NULL DEFAULT 0 CHECK (first_day_of_week BETWEEN 0 AND 6), -- chrono num_days_from_monday; 0 = Monday
    calendar_view TEXT NOT NULL DEFAULT 'month' CHECK (calendar_view IN ('week', 'month', 'year'))
);
INSERT INTO app_state (id) VALUES (1);
CREATE TABLE accounts (
    id TEXT PRIMARY KEY NOT NULL,       -- EventKit Account identifier
    position INTEGER,                   -- user's drag order; NULL = not ordered (follows by name)
    color TEXT,                         -- NULL = default_for(id); palette name ('red', ...) or '#rrggbb'
    alias TEXT,                         -- NULL = no Alias
    disabled BOOLEAN NOT NULL DEFAULT FALSE,
    hidden BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE TABLE calendars (
    id TEXT PRIMARY KEY NOT NULL,       -- EventKit Calendar identifier
    disabled BOOLEAN NOT NULL DEFAULT FALSE
);
CREATE TABLE folded_sections (
    section TEXT PRIMARY KEY NOT NULL CHECK (section IN ('calendars'))
);
