# crumble

Native desktop app built with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://crates.io/crates/gpui-kit).

## Run

Install Rust 1.96 through `rustup`, and Xcode with the macOS 26 SDK (Xcode 27 / Swift 6.4 is what the repository builds with). `crumble-transcribe` and `crumble-summarize` compile their Swift bridges with `xcrun swiftc` during the build ([ADR 0006](docs/adr/0006-on-device-transcription-via-swift-bridge.md)); if `xcrun` points at an older toolchain, select Xcode with `sudo xcode-select -s /Applications/Xcode.app`. `crumble-transcribe` also builds transcribe.cpp's C++ for Local Models, which needs CMake (`brew install cmake`) ([ADR 0010](docs/adr/0010-local-models-through-transcribe-cpp-and-handys-list.md)). Then:

```sh
cargo run -p crumble --locked
```

On macOS, `cargo run` wraps the app in `target/<profile>/Crumble.app` and launches it with `open`, so macOS can grant it calendar access (see `scripts/macos-runner.sh`). Set `CRUMBLE_SIGNING_IDENTITY` to one of your code signing identities (`security find-identity -v -p codesigning`) to keep that permission across rebuilds. Without it, the bundle is signed ad hoc and macOS asks again after every code change. A build signed differently also replaces the permission, so set the identity once in `~/.cargo/config.toml` rather than per shell, and every `cargo run` uses it:

```toml
[env]
CRUMBLE_SIGNING_IDENTITY = "Apple Development: Your Name (TEAMID)"
```

## Architecture

Crumble is split into small crates on one line: whether code needs GPUI. Domain logic lives in crates without GPUI, so it is tested without building the UI, and `crumble-ui` holds the views. [ADR 0002](docs/adr/0002-crates-split-on-gpui-boundary.md) records the decision and [AGENTS.md](AGENTS.md) the rules for where code goes. [Native architecture](docs/architecture/native.md) describes behavior and which crate owns it, and [CONTEXT.md](CONTEXT.md) defines the domain terms.

## Build and verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
python3 scripts/check-crate-boundaries.py
```

CI (`.github/workflows/ci.yml`) runs these four on macOS 26 for every pull request and every push to `main`.

PR-Agent (`.github/workflows/pr-agent.yml`) reviews each PR with Claude when it opens or leaves draft, and posts code suggestions; it leaves the PR body alone. A collaborator's `/review`, `/improve` or `/ask …` comment runs it again, and is the only way to review a fork's PR, since GitHub withholds the API key from fork runs.

Run `git config core.hooksPath .githooks` once per clone to turn on the pre-commit hook. It rejects a commit that stages a file over 20,000 bytes and explains how to split it. Lockfiles, generated code, vendored skills and binary files are exempt; `.gitattributes` lists the exempt paths as `linguist-generated` or `linguist-vendored`.

`cargo test -p crumble-transcribe --locked -- --ignored` also transcribes speech made with `say` through Apple's model, downloading the English model the first time.

`cargo test -p crumble-summarize --locked -- --ignored --nocapture` summarizes an invented Transcript with Apple's on-device model (Apple Intelligence must be on) and with a local Ollama if one is running. `cargo test -p crumble-store --locked -- --ignored` saves, reads and removes a made-up key in the login Keychain.

## Changing the database schema

Crumble keeps its state in SQLite through [Diesel](https://diesel.rs/) ([ADR 0003](docs/adr/0003-app-state-in-sqlite-via-diesel.md)). The app runs pending migrations itself at startup, so normal runs need no Diesel CLI. To change the schema, install the CLI against macOS's SQLite:

```sh
cargo install diesel_cli --no-default-features --features sqlite
```

Then, from `crates/db` (where `diesel.toml` lives), add a migration, fill in its `up.sql` and `down.sql`, and regenerate `src/schema.rs` against a scratch database:

```sh
cd crates/db
diesel migration generate <name>
diesel migration run --database-url /tmp/crumble-scratch.sqlite3
rm /tmp/crumble-scratch.sqlite3
```

`migration run` rewrites `src/schema.rs`; commit it with the migration.

Icons in `assets/hugeicons` are from [Hugeicons](https://hugeicons.com/): the Stroke Rounded ones under the MIT license, and the Solid Rounded ones in `assets/hugeicons/solid`, which the rail shows for the selected page, from Hugeicons Pro.
