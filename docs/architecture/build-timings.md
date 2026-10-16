# Build timings

Dev-build times before and after the crate split in [ADR 0002](../adr/0002-crates-split-on-gpui-boundary.md). The split is justified by testability and enforced boundaries, not speed; these numbers check that it does not make builds slower and show what re-enabling `incremental` would buy.

## Machine

| | |
| --- | --- |
| Machine | Mac16,7, Apple M4 Pro, 14 cores, 48 GB RAM, macOS 26.7.1 |
| Toolchain | rustc 1.96.0 (ac68faa20 2026-05-25), aarch64-apple-darwin, LLVM 22.1.2 |
| Before | `2082890` on `main`, measured on a child commit that changes only `CONTEXT.md`: one `crumble-ui` crate |
| After | `2e75220`, the `crumble-sync` layer before its review fixes (which touch only `crumble-store` internals): `crumble-calendar`, `crumble-store`, `crumble-sync` split out of `crumble-ui` |
| Profile | `dev` from `Cargo.toml`: `debug = 0`, `incremental = false`, dependencies at `opt-level = 2` |

## Results

Wall time in seconds, median of 3 runs (A: one run). "incremental on" sets `CARGO_PROFILE_DEV_INCREMENTAL=true` in the environment; `Cargo.toml` is unchanged. Neither set of runs was isolated from other work on the machine, so differences under about 0.1 s are noise.

| Scenario | off: before | off: after | off: delta | on: before | on: after | on: delta |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| A. Clean build, `cargo build --workspace` | 99.07 | 97.46 | -1.61 | not measured | 88.70 | |
| B. View edit, rebuild `-p crumble` | 2.34 | 2.08 | -0.26 | 1.84 | 1.75 | -0.09 |
| C. Logic edit, rebuild `-p crumble` | 2.31 | 2.36 | +0.05 | 1.77 | 2.11 | +0.34 |
| D. Logic edit, `cargo test --workspace --no-run` | 2.19 | 2.26 | +0.07 | 1.63 | 2.20 | +0.57 |
| E. Logic edit, Period tests alone | 2.45 | 0.73 | -1.72 | 1.90 | 0.76 | -1.14 |
| F. Store edit, `cargo test -p crumble-store` | | 0.99 | | | 0.97 | |

E ran `cargo test -p crumble-ui --lib -- period` before the split and `cargo test -p crumble-calendar -- period` after. F is new: before the split that code sat in `crumble-ui`, so its nearest "before" number is E's 2.45 s. The incremental-on clean build was one extra run without `--timings`, outside the script.

Per-crate compile time in the clean build (A), from `target/cargo-timings/cargo-timing.html` (604 units before, 638 after; the rest is dependencies):

| Crate | Before | After |
| --- | ---: | ---: |
| `crumble-ui` | 2.76 | 2.01 |
| `crumble` (bin) | 1.52 | 1.12 |
| `crumble-sync` | | 1.11 |
| `crumble-store` | | 0.99 |
| `crumble-eventkit` | 0.68 | 0.64 |
| `crumble-calendar` | | 0.58 |
| `crumble-theme` | 0.28 | 0.17 |

Disk use of `target` after the script (clean build, then every scenario): 2.0 GB with incremental off, 2.7 GB once the incremental-on runs have added their caches. A clean incremental-on build alone leaves 1.7 GB.

### Conclusion

The domain test loops got fast, and nothing else moved much:

- **Faster**: Period tests (E) dropped from 2.45 s to 0.73 s, and store tests (F) run in about 1 s, because neither builds GPUI or `crumble-ui` any more. This is the payoff the split was for.
- **Slightly faster**: a view edit (B) rebuilds a smaller `crumble-ui`, 2.34 s to 2.08 s.
- **Unchanged**: a logic edit that rebuilds the app (C) or the test binaries (D) costs the same with incremental off. The crumble crates still form a chain (`crumble-eventkit` → `crumble-calendar` → `crumble-store`, `crumble-sync` → `crumble-ui` → `crumble`), so an edit in `crumble-calendar` now recompiles it, `crumble-store`, `crumble-sync` and `crumble-ui` before relinking the app, where before only `crumble-ui` recompiled. Their smaller sizes add up to about what `crumble-ui` alone cost.
- **Slower with incremental on**: C and D lost most of what incremental used to save (1.77 s to 2.11 s, 1.63 s to 2.20 s). Incremental reuses work inside a crate; before the split the whole edit sat in one crate that it could mostly reuse, and now each downstream crate pays its own fixed cost on every edit.
- **Clean builds** are dominated by dependencies and did not change.

### Recommendation on `incremental`

Keep `incremental = false`. Before the split, turning it on saved about 0.5 s on every edit loop; now it saves 0.33 s on a view edit (B), 0.25 s on a logic edit (C), and nothing on D, E or F, where the difference is within noise. In exchange it costs about 0.7 GB more `target` (2.0 GB to 2.7 GB after the scenarios above), growing with each edited build until `cargo clean`. Clean builds are not affected: Cargo uses incremental only for workspace members and path dependencies, not for the 600-odd crates.io and git units that dominate a clean build, and the single incremental-on clean build (88.70 s) came out no slower than the default. The `gpui-mcp` patch of `gpui-pre` is a git dependency, so incremental does not apply to it either way, and `--features mcp` builds behave the same. Switching back is a one-line change to `[profile.dev]` if view editing becomes the loop that matters; it needs separate approval.

### Raw numbers

Before the split:

| Scenario | off: run 1 | run 2 | run 3 | on: run 1 | run 2 | run 3 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| B | 2.34 | 2.43 | 2.33 | 1.84 | 1.84 | 1.76 |
| C | 2.22 | 2.37 | 2.31 | 1.81 | 1.77 | 1.77 |
| D | 5.12 | 2.18 | 2.19 | 1.62 | 1.63 | 1.67 |
| E | 2.56 | 2.45 | 2.43 | 1.90 | 1.96 | 1.89 |

The 5.12 s for D, run 1 is an outlier the median drops.

After the split:

| Scenario | off: run 1 | run 2 | run 3 | on: run 1 | run 2 | run 3 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| B | 2.08 | 2.13 | 2.00 | 1.72 | 1.76 | 1.75 |
| C | 2.39 | 2.36 | 2.35 | 2.11 | 2.09 | 2.11 |
| D | 2.26 | 2.24 | 2.26 | 2.12 | 2.20 | 2.20 |
| E | 0.73 | 0.77 | 0.68 | 0.76 | 0.75 | 0.80 |
| F | 0.99 | 0.99 | 1.09 | 0.96 | 0.98 | 0.97 |

## Protocol

Each scenario edits a source file so that it really changes, then times one Cargo command with `/usr/bin/time -p`. The edit restores the file from `HEAD` and appends `const _: u32 = <n>;` with a fresh `<n>` for each run.

- **A. Clean build**: `cargo clean`, then `cargo build --workspace --locked --timings`. One run. Per-crate times come from the `UNIT_DATA` array in `target/cargo-timings/cargo-timing.html`.
- **B. View edit**: probe `crates/ui/src/native_view/shell.rs` (the app shell view), then `cargo build -p crumble --locked`.
- **C. Logic edit**: probe the file holding Period math and today, then `cargo build -p crumble --locked`. At the baseline it was `crates/ui/src/native_view/calendar/period.rs`; it is now `crates/calendar/src/period.rs`.
- **D. Test build after a logic edit**: the C probe, then `cargo test --workspace --locked --no-run`.
- **E. Pure-logic test loop**: the C probe, then the Period tests alone. At the baseline they compiled inside the GPUI crate: `cargo test -p crumble-ui --locked --lib -- period`. They now run in their own crate: `cargo test -p crumble-calendar --locked -- period`.
- **F. Store test loop** (added after the split): probe `crates/store/src/account_color.rs`, then `cargo test -p crumble-store --locked`.

B to F run three times each with `incremental` off, then again with `CARGO_PROFILE_DEV_INCREMENTAL=true`. Before each mode, one untimed warm build fills the cache, and D, E and F each get an untimed warm run first. `cargo build` is used instead of `cargo run`, which would launch the macOS runner.

### How to reproduce

From the repository root, on a clean tree, run the script below; it refuses to start on a dirty tree, prints one line per timed run (`<mode> <scenario> <run> <seconds>`) and leaves the tree as it found it. Update `VIEW`, `LOGIC`, `STORE`, and the E command to wherever the code lives when you run it. Close other heavy work first.

```sh
#!/bin/bash
set -euo pipefail
VIEW=crates/ui/src/native_view/shell.rs
LOGIC=crates/calendar/src/period.rs
STORE=crates/store/src/account_color.rs
OUT=${OUT:-/tmp/crumble-timings}
git diff --quiet HEAD || { echo "commit or stash your changes first" >&2; exit 1; }
trap 'git checkout -- "$VIEW" "$LOGIC" "$STORE"' EXIT # restore the probe files even if a build fails
unset CARGO_PROFILE_DEV_INCREMENTAL # A runs with the profile as committed
mkdir -p "$OUT"
n=0
probe() { # append a fresh `const _: u32 = <n>;` to the file so it really changes
  n=$((n + 1)); git show "HEAD:$1" > "$1"; echo "const _: u32 = $n;" >> "$1"
}
timed() { # timed <label> <cmd...>: log wall time in seconds
  local label=$1; shift
  /usr/bin/time -p "$@" > /dev/null 2> "$OUT/last.err" || { cat "$OUT/last.err"; exit 1; }
  echo "$label $(awk '/^real/ {print $2}' "$OUT/last.err")" | tee -a "$OUT/results.txt"
}
: > "$OUT/results.txt"

cargo clean
timed "A clean" cargo build --workspace --locked --timings
cp target/cargo-timings/cargo-timing.html "$OUT/"

for mode in off on; do
  if [ $mode = on ]; then export CARGO_PROFILE_DEV_INCREMENTAL=true; else unset CARGO_PROFILE_DEV_INCREMENTAL; fi
  cargo build -p crumble --locked > /dev/null 2>&1
  for i in 1 2 3; do probe $VIEW;  timed "$mode B $i" cargo build -p crumble --locked; done
  git checkout -- $VIEW
  for i in 1 2 3; do probe $LOGIC; timed "$mode C $i" cargo build -p crumble --locked; done
  cargo test --workspace --locked --no-run > /dev/null 2>&1
  for i in 1 2 3; do probe $LOGIC; timed "$mode D $i" cargo test --workspace --locked --no-run; done
  cargo test -p crumble-calendar --locked -- period > /dev/null 2>&1
  for i in 1 2 3; do probe $LOGIC; timed "$mode E $i" cargo test -p crumble-calendar --locked -- period; done
  git checkout -- $LOGIC
  cargo test -p crumble-store --locked > /dev/null 2>&1
  for i in 1 2 3; do probe $STORE; timed "$mode F $i" cargo test -p crumble-store --locked; done
  git checkout -- $STORE
  du -sh target | sed "s/^/$mode du /" | tee -a "$OUT/results.txt"
done
git diff --exit-code
```

Record the machine with `sysctl -n hw.model machdep.cpu.brand_string hw.ncpu hw.memsize` and `rustc -Vv`, and the commit with `git rev-parse HEAD`.
