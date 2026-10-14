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

## Build and verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

See [native architecture](docs/architecture/native.md) for the package layout and app shell.

Icons in `assets/hugeicons` are from [Hugeicons](https://hugeicons.com/) under the MIT license.
