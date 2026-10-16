# Crumble

Crumble is a macOS desktop calendar app built with GPUI and GPUI Kit that reads Events through EventKit.

- [`CONTEXT.md`](CONTEXT.md) is the domain glossary. Use its terms in code, docs, and commits.
- [`docs/architecture/native.md`](docs/architecture/native.md) describes behavior and which crate owns what.
- [`docs/adr/`](docs/adr/) records decisions. Read the relevant ADR before changing what it decided.

## Crates

- `apps/desktop` (`crumble`): native lifecycle, window, menus, key bindings.
- `crates/ui` (`crumble-ui`): GPUI views and presentation state.
- `crates/calendar` (`crumble-calendar`): Calendar views and Period math, Event placement, Agenda rows and read planning, description links. No GPUI, no IO.
- `crates/store` (`crumble-store`): state kept across restarts in `settings.json`, and Account rules: order, color, Alias, label, Disabled and Hidden filtering. No GPUI.
- `crates/theme` (`crumble-theme`): colors, typography, dimensions.
- `crates/eventkit` (`crumble-eventkit`): read-only EventKit bridge, plain Rust data, no GPUI.

The split of non-GPUI logic out of `crumble-ui` is in progress per [ADR 0002](docs/adr/0002-crates-split-on-gpui-boundary.md). Until it finishes, `crumble-ui` still holds domain logic and persistence that the ADR assigns to other crates.

## Where code goes

Split on one question: does this code need GPUI?

- Code that needs no GPUI goes in a domain crate: time and Period math, Event placement, Account rules, persistence.
- `crumble-ui` holds views and short-lived presentation state: focus, scroll, hover, motion counters, redraw timers.
- `crumble-ui` reads and changes domain state only through domain crates. It holds no domain rules, no file or database IO, and no persistence calls.
- Presentation-only pure helpers (route history, row reordering, menu stepping, sidebar sizing) stay in `crumble-ui` as GPUI-free modules with plain `#[test]`s.
- Keep `gpui-kit` and the `mcp` feature out of domain crates.

## New crates

The gpui-kit Coding Guides (`.claude/skills/gpui-kit/references/coding-guides.md`) set the bar:

> Do not create a crate for every screen or helper. Split where a capability has its own state and lifecycle, a stable public seam, or enough implementation to benefit from independent compilation and tests.

- Lay crates out flat as `crates/<name>/`, package `crumble-<name>`, registered in root `[workspace.dependencies]`.
- Put a shared type in the lowest domain crate that owns it. Name crates after a capability; `common`, `utils`, or `types` crates hide ownership.
- Keep the dependency graph acyclic, pointing toward smaller, more stable crates.

## Rust conventions

- Use concrete types and functions. Add a trait only when it has two real implementations.
- Keep crate APIs narrow: `pub(crate)` by default, `pub` only for items another crate uses, each with a doc comment.
- Take dependency versions from `[workspace.dependencies]` and set `[lints] workspace = true`.
- Only `crumble-eventkit` uses `unsafe`, under its own lint table ([ADR 0001](docs/adr/0001-read-only-events-via-eventkit.md)). Every other crate keeps `unsafe_code = "forbid"`.

## Testing

- Test pure logic with plain `#[test]` in GPUI-free crates or modules.
- Use `TestAppContext` only for behavior that needs GPUI: entities, focus, actions, rendering.

## Build and verify

Run all three before you call work done:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

`cargo run -p crumble --locked` launches `Crumble.app` through `scripts/macos-runner.sh`; see the README for signing.

## Docs

- A logic change updates the sections of `docs/architecture/native.md` that describe it, including the Ownership table.
- A new domain term goes into `CONTEXT.md`. "Settings" means only the Settings page.
- A new crate goes into the Crates list above.
