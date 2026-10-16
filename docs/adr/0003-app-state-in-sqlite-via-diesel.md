# App state in SQLite through Diesel

State Crumble keeps across restarts (the First day of week, the Calendar view, and each Account's order, color, Alias, Disabled and Hidden choices, Disabled Calendars, and folded sidebar sections) moves from `settings.json` to a SQLite database, `crumble.sqlite3` in `~/Library/Application Support/Crumble`, accessed through Diesel. Planned features will need real queries over stored data, and one storage approach for all app state is simpler than a JSON file per feature. `crumble-store` loads every table into memory at startup and writes each change through `Store` methods; views never persist.

## Considered Options

- **Keep JSON**: simple, but it has no queries, every change rewrites the whole file, and it does not scale to future state.
- **rusqlite**: lighter, but it has no compile-time-checked schema or query DSL.
- **sqlx**: async-first, while Crumble's writes are synchronous one-row updates on the main thread.
- **Diesel** (chosen): a compile-time-checked schema, embedded migrations run at startup, and a synchronous API.

## Consequences

- There is no import from `settings.json`. Users start over on the defaults, and the old file is left untouched.
- Writes are synchronous on the main thread. They are one-row updates, so the Alias field can save on every keystroke; if they ever show up in frame times, the upgrade is a background writer.
- Diesel links macOS's system SQLite, with no bundled C build, so ADR 0001's macOS-only scope covers it.
- Schema changes need the Diesel CLI: a new migration plus a regenerated `schema.rs`, both checked in. The README has the commands.
- If the database cannot be opened, migrated, or read, the app logs it and runs on in-memory defaults without touching the file. A failed write is logged, and the value stays in memory.
- Shared file: `crumble-db` owns `crumble.sqlite3`, every migration and the schema; each domain crate keeps its own connection and its own queries. Files run in WAL mode with a 5 s busy timeout, so one connection reads while another writes.
