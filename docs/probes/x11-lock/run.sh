#!/usr/bin/env bash
# run.sh <none|xfwm4|xfwm4-comp> [PROBE_VAR=value...]
# Builds the probe, then runs scenario.sh on a private Xvfb display with a
# private D-Bus session and no inherited desktop variables. Output goes to
# target/x11-probe/out-<wm>[-<variant>] in the repository.
set -u
HERE=$(cd "$(dirname "$0")" && pwd)
ROOT=$(cd "$HERE/../../.." && pwd)
TARGET=$ROOT/target/x11-probe
WM=$1
shift
CARGO_TARGET_DIR=$TARGET cargo build --manifest-path "$HERE/Cargo.toml" || exit 1
EXTRA="$*"
OUT=$TARGET/out-$WM${EXTRA:+-${EXTRA//[^A-Za-z0-9]/_}}
rm -rf "$OUT" && mkdir -p "$OUT"
RUNTIME=$(mktemp -d "$TARGET/runtime.XXXX") && chmod 700 "$RUNTIME"
# -extension Composite in XVFB_EXTRA reproduces the VisibilityNotify comparison.
env -i HOME="$HOME" PATH=/usr/bin:/bin USER="$USER" LANG=C.UTF-8 XDG_RUNTIME_DIR="$RUNTIME" GDK_DEBUG=no-portals "$@" \
    timeout 150 xvfb-run -a -s "-screen 0 1280x800x24 -nolisten tcp ${XVFB_EXTRA:-}" \
    dbus-run-session -- bash "$HERE/scenario.sh" "$OUT" "$WM" "$TARGET/debug" > "$OUT/run.log" 2>&1
code=$?
rm -rf "$RUNTIME"
grep -E '^(==|LOCK (start|report|grab|entry|unlocked|focus|popover|fps)|INTRUDER|raise-reactions)' "$OUT/run.log"
echo "RUN EXIT: $code (full log: $OUT/run.log)"
exit $code
