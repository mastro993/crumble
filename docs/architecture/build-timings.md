# Build timings

Baseline dev-build times before the crate split in [ADR 0002](../adr/0002-crates-split-on-gpui-boundary.md). The split is justified by testability and enforced boundaries, not speed; these numbers check that it does not make builds slower and show what re-enabling `incremental` would buy. When the split is done, the same protocol runs again and adds an "after" column.

## Machine

| | |
| --- | --- |
| Machine | Mac16,7, Apple M4 Pro, 14 cores, 48 GB RAM, macOS 26.7.1 |
| Toolchain | rustc 1.96.0 (ac68faa20 2026-05-25), aarch64-apple-darwin, LLVM 22.1.2 |
| Commit | `2082890` on `main`, measured on a child commit that changes only `CONTEXT.md` |
| Profile | `dev` from `Cargo.toml`: `debug = 0`, `incremental = false`, dependencies at `opt-level = 2` |

## Results

Wall time in seconds, median of 3 runs (A: one run). "incremental on" sets `CARGO_PROFILE_DEV_INCREMENTAL=true` in the environment; `Cargo.toml` is unchanged.

| Scenario | incremental off (default) | incremental on |
| --- | ---: | ---: |
| A. Clean build, `cargo build --workspace` | 99.07 | not measured |
| B. View edit, rebuild `-p crumble` | 2.34 | 1.84 |
| C. Logic edit, rebuild `-p crumble` | 2.31 | 1.77 |
| D. Logic edit, `cargo test --workspace --no-run` | 2.19 | 1.63 |
| E. Logic edit, `cargo test -p crumble-ui --lib -- period` | 2.45 | 1.90 |

Per-crate compile time in the clean build (A), from `target/cargo-timings/cargo-timing.html` (604 units in total; the rest is dependencies):

| Crate | Seconds |
| --- | ---: |
| `crumble-ui` | 2.76 |
| `crumble` (bin) | 1.52 |
| `crumble-eventkit` | 0.68 |
| `crumble-theme` | 0.28 |

Today the crumble crates form a chain (`crumble-eventkit`, `crumble-theme` → `crumble-ui` → `crumble`), so any edit in `crumble-ui` rebuilds it and relinks the app. That is why B and C cost the same: the view file and the logic file sit in the same crate.

### Raw numbers

| Scenario | off: run 1 | run 2 | run 3 | on: run 1 | run 2 | run 3 |
| --- | ---: | ---: | ---: | ---: | ---: | ---: |
| B | 2.34 | 2.43 | 2.33 | 1.84 | 1.84 | 1.76 |
| C | 2.22 | 2.37 | 2.31 | 1.81 | 1.77 | 1.77 |
| D | 5.12 | 2.18 | 2.19 | 1.62 | 1.63 | 1.67 |
| E | 2.56 | 2.45 | 2.43 | 1.90 | 1.96 | 1.89 |

The 5.12 s for D, run 1 is an outlier the median drops; the runs were not isolated from other work on the machine.

## Protocol

Each scenario edits a source file so that it really changes, then times one Cargo command with `/usr/bin/time -p`. The edit restores the file from `HEAD` and appends `const _: u32 = <n>;` with a fresh `<n>` for each run.

- **A. Clean build**: `cargo clean`, then `cargo build --workspace --locked --timings`. One run. Per-crate times come from the `UNIT_DATA` array in `target/cargo-timings/cargo-timing.html`.
- **B. View edit**: probe `crates/ui/src/native_view/shell.rs` (the app shell view), then `cargo build -p crumble --locked`.
- **C. Logic edit**: probe the file holding Period math and today, then `cargo build -p crumble --locked`. At the baseline it is `crates/ui/src/native_view/calendar/period.rs`; once that code moves to `crumble-calendar`, probe its new file.
- **D. Test build after a logic edit**: the C probe, then `cargo test --workspace --locked --no-run`.
- **E. Pure-logic test loop**: the C probe, then the Period tests alone. At the baseline they compile inside the GPUI crate: `cargo test -p crumble-ui --locked --lib -- period`. After the split, run them in the crate that holds them.

B to E run three times each with `incremental` off, then again with `CARGO_PROFILE_DEV_INCREMENTAL=true`. Before each mode, one untimed warm build fills the cache, and D and E each get an untimed warm run first. `cargo build` is used instead of `cargo run`, which would launch the macOS runner.

### How to reproduce

From the repository root, on a clean tree, run the script below; it refuses to start on a dirty tree, prints one line per timed run (`<mode> <scenario> <run> <seconds>`) and leaves the tree as it found it. Update `VIEW`, `LOGIC`, and the E command to wherever the code lives when you run it. Close other heavy work first.

```sh
#!/bin/bash
set -euo pipefail
VIEW=crates/ui/src/native_view/shell.rs
LOGIC=crates/ui/src/native_view/calendar/period.rs
OUT=${OUT:-/tmp/crumble-timings}
git diff --quiet HEAD || { echo "commit or stash your changes first" >&2; exit 1; }
trap 'git checkout -- "$VIEW" "$LOGIC"' EXIT # restore the probe files even if a build fails
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
  cargo test -p crumble-ui --locked --lib -- period > /dev/null 2>&1
  for i in 1 2 3; do probe $LOGIC; timed "$mode E $i" cargo test -p crumble-ui --locked --lib -- period; done
  git checkout -- $LOGIC
done
git diff --exit-code
```

Record the machine with `sysctl -n hw.model machdep.cpu.brand_string hw.ncpu hw.memsize` and `rustc -Vv`, and the commit with `git rev-parse HEAD`.
