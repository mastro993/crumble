CREATE TABLE folded_sections (
    section TEXT PRIMARY KEY NOT NULL CHECK (section IN ('calendars'))
);
