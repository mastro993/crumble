# crumble

Native desktop app built with [GPUI](https://www.gpui.rs/) and [GPUI Kit](https://crates.io/crates/gpui-kit).

## Run

Install Rust 1.96 through `rustup`, then:

```sh
cargo run -p crumble --locked
```

## Build and verify

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
```

See [native architecture](docs/architecture/native.md) for the package layout and app shell.

Icons in `assets/hugeicons` are from [Hugeicons](https://hugeicons.com/) under the MIT license.
