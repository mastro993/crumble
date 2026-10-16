# crumble

Native desktop app built with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://crates.io/crates/gpui-kit).

## Run

Install Rust 1.96 through `rustup`, then:

```sh
cargo run -p crumble --locked
```

On macOS, `cargo run` wraps the app in `target/<profile>/Crumble.app` and launches it with `open`, so macOS can grant it calendar access (see `scripts/macos-runner.sh`). Set `CRUMBLE_SIGNING_IDENTITY` to one of your code signing identities (`security find-identity -v -p codesigning`) to keep that permission across rebuilds. Without it, the bundle is signed ad hoc and macOS asks again after every code change. A build signed differently also replaces the permission, so set the identity once in `~/.cargo/config.toml` rather than per shell, and every `cargo run` uses it:

```toml
[env]
CRUMBLE_SIGNING_IDENTITY = "Apple Development: Your Name (TEAMID)"
```

## Architecture

Crumble is split into small crates on one line: whether code needs GPUI. Domain logic lives in crates without GPUI, so it is tested without building the UI, and `crumble-ui` holds the views; the split is still in progress. [ADR 0002](docs/adr/0002-crates-split-on-gpui-boundary.md) records the decision and [AGENTS.md](AGENTS.md) the rules for where code goes. [Native architecture](docs/architecture/native.md) describes behavior and which crate owns it, and [CONTEXT.md](CONTEXT.md) defines the domain terms.

## Build and verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

## Changing the database schema

Crumble keeps its state in SQLite through [Diesel](https://diesel.rs/) ([ADR 0003](docs/adr/0003-app-state-in-sqlite-via-diesel.md)). The app runs pending migrations itself at startup, so normal runs need no Diesel CLI. To change the schema, install the CLI against macOS's SQLite:

```sh
cargo install diesel_cli --no-default-features --features sqlite
```

Then, from `crates/store` (where `diesel.toml` lives), add a migration, fill in its `up.sql` and `down.sql`, and regenerate `src/schema.rs` against a scratch database:

```sh
cd crates/store
diesel migration generate <name>
diesel migration run --database-url /tmp/crumble-scratch.sqlite3
rm /tmp/crumble-scratch.sqlite3
```

`migration run` rewrites `src/schema.rs`; commit it with the migration.

Icons in `assets/hugeicons` are from [Hugeicons](https://hugeicons.com/) under the MIT license.
