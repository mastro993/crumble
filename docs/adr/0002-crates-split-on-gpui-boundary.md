# Crates split on the GPUI boundary

`crumble-ui` holds the views together with everything else Crumble does without EventKit: Period math, Event placement, Agenda planning, Account rules, and the file that keeps state across restarts. Its pure tests compile against GPUI, and nothing stops a view from doing IO or applying a domain rule inline. We split code into crates on one question: does it need GPUI? Code that does not moves into domain crates, and `crumble-ui` keeps the views, their short-lived presentation state, and presentation-only pure helpers. The target layout:

- `crumble-calendar`: time and Period math, Event placement, and Agenda planning. No GPUI, no IO.
- `crumble-store`: state kept across restarts, and Account rules (order, color, Alias, label, which Calendars are shown). No GPUI.
- `crumble-sync`: the GPUI entities that read EventKit (`CalendarSync`, the Events and Agenda loaders). Depends on GPUI core (`gpui-pre`) only, never on gpui-kit components.
- `crumble-ui`: views and presentation state.
- `crumble-theme` and `crumble-eventkit`: unchanged.

Domain logic then gets tests that do not build GPUI, and the crate graph enforces the boundaries: only `crumble-ui` knows gpui-kit components, and only `crumble-eventkit` knows objc2. This matches how Zed lays out its workspace, with model crates such as `project` and `settings` separate from views and pure crates such as `rope` and `text` free of GPUI, and rust-analyzer's use of crates as API boundaries.

## Considered Options

- **Keep one `ui` crate**: rejected. Its pure tests compile against GPUI, and nothing stops views from doing IO.
- **One crate per feature or screen**: rejected for about 9k lines. The gpui-kit Coding Guides advise against it ("Do not create a crate for every screen or helper"). Each crate adds a manifest, a test binary, and link time, and the build graph here is a linear chain, so there is no parallelism to gain.
- **Split by role (all models in one crate, all views in another) or by a literal reading of SOLID**: rejected. The line that pays for itself is GPUI or not.
- **Abstract the executor so sync needs no GPUI**: rejected. It would hand-roll cancellation and threading that GPUI's tasks already provide, for no gain.

## Consequences

- Build speed changes little. Editing a view rebuilds a smaller `crumble-ui`, but editing domain logic still rebuilds `crumble-ui` after its domain crate, because the graph is a chain. Measured in [build timings](../architecture/build-timings.md): domain test loops dropped from 2.45 s to under 1 s, a view edit from 2.34 s to 2.08 s, and app or test rebuilds after a logic edit stayed the same. Re-enabling `incremental` now saves at most 0.33 s for 0.7 GB more `target`; the doc recommends keeping it off, and changing it needs separate approval.
- Items that cross a crate boundary become `pub` and need doc comments. Crate APIs stay narrow and concrete, without traits that have a single implementation.
- The orphan rule may force some trait impls to live in the crate that owns the type.
- State kept across restarts moves from `settings.json` to SQLite; [ADR 0003](0003-app-state-in-sqlite-via-diesel.md) records that decision.

## Sources

- [gpui-kit Coding Guides](https://gpui-kit.com/docs/coding-guides.md) (also `.claude/skills/gpui-kit/references/coding-guides.md`)
- matklad, [Large Rust Workspaces](https://matklad.github.io/2021/08/22/large-rust-workspaces.html)
- matklad, [Fast Rust Builds](https://matklad.github.io/2021/09/04/fast-rust-builds.html)
- [rust-analyzer architecture](https://rust-analyzer.github.io/book/contributing/architecture.html)
- The Rust Performance Book, [Compile Times](https://nnethercote.github.io/perf-book/compile-times.html)
- [Rust compiler performance survey 2025 results](https://blog.rust-lang.org/2025/09/10/rust-compiler-performance-survey-2025-results)
