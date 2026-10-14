#!/bin/sh
# Cargo runner on macOS (see .cargo/config.toml).
#
# macOS only grants calendar access to an app bundle whose Info.plist explains why. A bare
# binary started from a terminal counts as the terminal itself and is denied without a prompt.
# So the `crumble` binary is wrapped in Crumble.app next to it and launched through `open`.
# Every other binary, such as test executables, runs directly.
set -eu

binary=$1
shift
if [ "$(basename "$binary")" != crumble ]; then
	exec "$binary" "$@"
fi

app="$(dirname "$binary")/Crumble.app"
mkdir -p "$app/Contents/MacOS"
cp "$binary" "$app/Contents/MacOS/crumble"
cp "$(dirname "$0")/../apps/desktop/Info.plist" "$app/Contents/Info.plist"
# Signing seals the Info.plist into the bundle. macOS ties the calendar permission to the
# signature: a certificate identity (CRUMBLE_SIGNING_IDENTITY) keeps it across rebuilds, while
# the ad-hoc fallback changes with every build, so macOS asks again after each code change.
identity=${CRUMBLE_SIGNING_IDENTITY:--}
if [ "$identity" = - ]; then
	echo "Signing Crumble.app ad hoc: macOS asks for calendar access again after this rebuild." >&2
	echo "Set CRUMBLE_SIGNING_IDENTITY to keep it (see README)." >&2
fi
if ! output=$(codesign --force --sign "$identity" "$app" 2>&1); then
	echo "$output" >&2
	exit 1
fi

# `open` makes Crumble its own process instead of a child of the terminal. Its output goes to
# this terminal when there is one.
log=$(tty 2>/dev/null) || log=/dev/null
open -W -n "$app" --stdout "$log" --stderr "$log" --args "$@" &
waiter=$!
# Ctrl-C only reaches this script and `open`, so quit the app too instead of leaving it running.
trap 'pkill -f "$app/Contents/MacOS/crumble"' INT TERM
wait "$waiter"
