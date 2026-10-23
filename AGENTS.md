# Crumble

Crumble is a macOS desktop calendar app built with GPUI and GPUI Kit that reads Events through EventKit.

- [`CONTEXT.md`](CONTEXT.md) is the domain glossary. Use its terms in code, docs, and commits.
- [`docs/architecture/native.md`](docs/architecture/native.md) says which crate owns what and links one page per area that describes its behavior.
- [`docs/adr/`](docs/adr/) records decisions. Read the relevant ADR before changing what it decided.

## Crates

- `apps/desktop` (`crumble`): native lifecycle, window, menus, key bindings.
- `crates/ui` (`crumble-ui`): GPUI views and presentation state.
- `crates/calendar` (`crumble-calendar`): Calendar views and Period math, Event placement, Agenda rows and read planning, description links. No GPUI, no IO.
- `crates/sync` (`crumble-sync`): GPUI entities that read EventKit: `CalendarSync`, `CalendarEvents`, `AgendaEvents`. GPUI core only, no `gpui-kit`.
- `crates/db` (`crumble-db`): the one SQLite file, its migrations, and the Diesel schema; `open_file()` and `open(url)`. No GPUI.
- `crates/store` (`crumble-store`): state kept across restarts, including each remote Provider's address and Models; Account rules: order, color, Alias, label, Disabled and Hidden filtering; and API keys in the Keychain through `secrets`. No GPUI.
- `crates/notes` (`crumble-notes`): Notes and Quick notes, how a Note finds its Event, and the folder each Recording lives in. No GPUI.
- `crates/audio` (`crumble-audio`): Recordings: microphone and process-tap capture into AAC files, echo removal from the microphone, and playback. No GPUI.
- `crates/transcribe` (`crumble-transcribe`): Transcripts from the `Transcriber` trait: Apple's SpeechAnalyzer on the Mac through a Swift bridge, Local Models through transcribe.cpp in a worker process, and remote Providers through the speech API each one's `Remote` descriptor names. No GPUI.
- `crates/local-models` (`crumble-local-models`): Local Models: Handy's model list copied in `handy/`, the models folder, and downloads from Hugging Face. No GPUI, no C++.
- `crates/summarize` (`crumble-summarize`): Summaries from the `Summarizer` trait: Apple's on-device Foundation Models through a Swift bridge, and remote Providers through the chat API each one's `Remote` descriptor names. No GPUI.
- `crates/remote` (`crumble-remote`): the Provider registry: `Provider`, `Job`, a `Remote` descriptor per remote Provider (address, API key format, APIs, suggested Models, key check), `ping`, `runs_locally`, and the HTTP helpers. No GPUI.
- `crates/notify` (`crumble-notify`): Suggestions: macOS notifications with a Record button through `UNUserNotificationCenter`, and the clicks on them. No GPUI.
- `crates/theme` (`crumble-theme`): colors, typography, dimensions.
- `crates/eventkit` (`crumble-eventkit`): read-only EventKit bridge, plain Rust data, no GPUI.

## Where code goes

Split on one question: does this code need GPUI?

- Code that needs no GPUI goes in a domain crate: time and Period math, Event placement, Account rules, persistence.
- `crumble-ui` holds views and short-lived presentation state: focus, scroll, hover, motion counters, redraw timers.
- `crumble-ui` reads and changes domain state only through domain crates. It holds no domain rules, no file or database IO, and no persistence calls.
- EventKit is read only through `crumble-sync`. `crumble-ui` uses `crumble-eventkit` for its types alone.
- Stored state changes only through `Store` and `Notes` methods, which write each change themselves. Views call those methods and never persist.
- Domain crates own their queries; `crumble-db` owns only the file, the migrations and the schema.
- A schema change is a new migration in `crates/db/migrations/` plus a regenerated `crates/db/src/schema.rs` ([ADR 0003](docs/adr/0003-app-state-in-sqlite-via-diesel.md); the README has the commands). A migration folder keeps its name forever: Diesel records it by that name.
- Presentation-only pure helpers (route history, row reordering, menu stepping, sidebar sizing) stay in `crumble-ui` as GPUI-free modules with plain `#[test]`s.
- Keep `gpui-kit` and the `mcp` feature out of domain crates.
- Link anything kept about an Event, such as a Note, by `series_uid` and `occurrence` ([ADR 0004](docs/adr/0004-event-identity-for-notes.md)). `Event::id` changes when the Event moves.
- Core Audio callbacks never block; hand buffers to the writer thread ([ADR 0005](docs/adr/0005-recording-with-a-process-tap.md)).
- Swift lives only in `crates/<crate>/swift/`, called through that crate's `src/ffi.rs` ([ADR 0006](docs/adr/0006-on-device-transcription-via-swift-bridge.md)).
- API keys go only through the Keychain module, `crumble_store::secrets`; keep them out of SQLite, logs, errors and UI text ([ADR 0007](docs/adr/0007-summaries-and-model-providers.md), [ADR 0011](docs/adr/0011-remote-providers-as-a-registry.md)).
- Providers: a remote Provider is one `Remote` descriptor in `crates/remote/src/remote/<id>.rs`, which Settings and the jobs read ([ADR 0011](docs/adr/0011-remote-providers-as-a-registry.md)). Add or change one with the `add-provider` skill, which follows [`docs/architecture/adding-a-provider.md`](docs/architecture/adding-a-provider.md).
- Crumble records only after the user's click. A Suggestion offers a Recording and starts one only from its Record button ([ADR 0009](docs/adr/0009-suggestions-through-user-notifications.md)).
- Audio leaves the Mac only through the consent check, `consent_needed` in `crates/ui/src/native_view/notes/consent.rs` ([ADR 0008](docs/adr/0008-remote-transcription-and-audio-consent.md)). Only Stop transcribes on its own; every other Transcript is asked for ([ADR 0010](docs/adr/0010-local-models-through-transcribe-cpp-and-handys-list.md)). Older Transcripts go to a Summary Model without a click only when `crumble_remote::runs_locally` says it is on the Mac.
- `crates/local-models/handy/catalog.json` is Handy's file, unchanged; `.github/workflows/handy-catalog.yml` syncs it. Never edit it by hand.

## New crates

The gpui-kit Coding Guides (`.claude/skills/gpui-kit/references/coding-guides.md`) set the bar:

> Do not create a crate for every screen or helper. Split where a capability has its own state and lifecycle, a stable public seam, or enough implementation to benefit from independent compilation and tests.

- Lay crates out flat as `crates/<name>/`, package `crumble-<name>`, registered in root `[workspace.dependencies]`.
- Put a shared type in the lowest domain crate that owns it. Name crates after a capability; `common`, `utils`, or `types` crates hide ownership.
- Keep the dependency graph acyclic, pointing toward smaller, more stable crates.

## Rust conventions

- Use concrete types and functions. Add a trait only when it has two real implementations.
- Keep crate APIs narrow: `pub(crate)` by default, `pub` only for items another crate uses, each with a doc comment.
- Take dependency versions from `[workspace.dependencies]` and set `[lints] workspace = true`. Member manifests must not pin versions; `scripts/check-crate-boundaries.py` checks this.
- A module with submodules is `foo.rs` plus a `foo/` folder, never `foo/mod.rs`; clippy's `mod_module_files` rejects `mod.rs`. A crate with its own lint table repeats that lint.
- Only `crumble-eventkit`, `crumble-audio`, `crumble-transcribe`, `crumble-summarize` and `crumble-notify` use `unsafe`, each under its own lint table ([ADR 0001](docs/adr/0001-read-only-events-via-eventkit.md), [ADR 0005](docs/adr/0005-recording-with-a-process-tap.md), [ADR 0006](docs/adr/0006-on-device-transcription-via-swift-bridge.md), [ADR 0007](docs/adr/0007-summaries-and-model-providers.md), [ADR 0009](docs/adr/0009-suggestions-through-user-notifications.md)). Every other crate keeps `unsafe_code = "forbid"`.

## Invariants CI enforces

`scripts/check-crate-boundaries.py` and `.github/workflows/ci.yml` fail a change that:

- Adds `gpui` or `gpui-pre` outside `crumble`, `ui`, `theme`, and `sync`, or `gpui-kit` to `sync` or any domain crate, directly or through one of those crates.
- Adds Diesel outside `db`, `store`, and `notes`.
- Adds objc2 crates outside `eventkit`, `audio`, and `notify`.
- Adds `transcribe-cpp` outside `transcribe` ([ADR 0010](docs/adr/0010-local-models-through-transcribe-cpp-and-handys-list.md)).
- Pins a crate version in a member `Cargo.toml` instead of `[workspace.dependencies]`.
- Drops `unsafe_code = "deny"` from a crate that uses `unsafe`, or `[lints] workspace = true` from any other.

## Testing

- Test pure logic with plain `#[test]` in GPUI-free crates or modules.
- Use `TestAppContext` only for behavior that needs GPUI: entities, focus, actions, rendering.

## Keyboard

Custom clickable elements that are not a GPUI Kit `Button` go through `native_view::keyboard::focusable` (or `sidebar_row::activate`) so Tab, Enter, and Space reach them. Repeated sibling elements need distinct `ElementId`s; do not key them by display name.

## Build and verify

Run all four before you call work done:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-crate-boundaries.py
```

The pre-commit hook in `.githooks/` rejects a staged file over 20,000 bytes. Split the file as its message says; do not bypass it with `--no-verify`.

`cargo run -p crumble --locked` launches `Crumble.app` through `scripts/macos-runner.sh`; see the README for signing.

## Docs

- A logic change updates the pages in `docs/architecture/` that describe it, including the Ownership table in `native.md`.
- A new domain term goes into `CONTEXT.md`. "Settings" means only the Settings page.
- A new crate goes into the Crates list above.
