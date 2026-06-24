#!/usr/bin/env bash
# pjdfstest conformance runner for libfuse-fs passthrough on macOS.
#
# Skips cleanly when macFUSE or pjdfstest aren't present; never blocks CI.
#
# Required env:
#   RUN_MACFUSE_CONFORMANCE  Must be 1; otherwise the script skips.
#   PJDFSTEST_DIR       Path to a checked-out github.com/pjd/pjdfstest tree
#                       (with `prove`-runnable tests under tests/).
# Optional env:
#   MOUNT_POINT         Override mountpoint (default: /tmp/libfusefs-conf-mnt)
#   ROOT_DIR            Override backing dir (default: /tmp/libfusefs-conf-root)
#   PASSTHROUGH_BIN     Override built example path
#                       (default: target/debug/examples/passthrough)
#   PROVE               Override prove(1) binary (default: prove)
#   KEEP_MOUNT          Set to 1 to leave mount/processes after run (debug)

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"

resolve_target_dir() {
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        echo "$CARGO_TARGET_DIR"
        return
    fi
    if command -v cargo >/dev/null 2>&1; then
        local d
        d="$(cargo metadata --no-deps --format-version 1 \
                --manifest-path "$REPO_ROOT/Cargo.toml" 2>/dev/null \
            | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])' 2>/dev/null)"
        if [[ -n "$d" ]]; then
            echo "$d"
            return
        fi
    fi
    echo "$REPO_ROOT/target"
}
TARGET_DIR="$(resolve_target_dir)"

skip() {
    echo "SKIP: $*" >&2
    exit 0
}

# --- env probes -------------------------------------------------------------

if [[ "$(uname -s)" != "Darwin" ]]; then
    skip "not running on macOS (uname=$(uname -s))"
fi
if [[ "${RUN_MACFUSE_CONFORMANCE:-0}" != "1" ]]; then
    skip "RUN_MACFUSE_CONFORMANCE!=1"
fi

MACFUSE_BIN="/Library/Filesystems/macfuse.fs/Contents/Resources/mount_macfuse"
if [[ ! -x "$MACFUSE_BIN" ]]; then
    skip "macFUSE not installed at $MACFUSE_BIN"
fi

if [[ -z "${PJDFSTEST_DIR:-}" ]]; then
    skip "PJDFSTEST_DIR not set; clone https://github.com/pjd/pjdfstest and point at it"
fi
if [[ ! -d "$PJDFSTEST_DIR/tests" ]]; then
    skip "PJDFSTEST_DIR=$PJDFSTEST_DIR does not contain tests/"
fi

PROVE="${PROVE:-prove}"
if ! command -v "$PROVE" >/dev/null 2>&1; then
    skip "prove(1) not found in PATH"
fi

PASSTHROUGH_BIN="${PASSTHROUGH_BIN:-$TARGET_DIR/debug/examples/passthrough}"
if [[ ! -x "$PASSTHROUGH_BIN" ]]; then
    echo "Building passthrough example…" >&2
    (cd "$REPO_ROOT" && cargo build --example passthrough)
fi

MOUNT_POINT="${MOUNT_POINT:-/tmp/libfusefs-conf-mnt}"
ROOT_DIR="${ROOT_DIR:-/tmp/libfusefs-conf-root}"

# --- setup ------------------------------------------------------------------

mkdir -p "$MOUNT_POINT" "$ROOT_DIR"
MOUNT_POINT_REAL="$(cd "$MOUNT_POINT" && pwd -P)"

cleanup() {
    local rc=$?
    if [[ "${KEEP_MOUNT:-0}" != "1" ]]; then
        if mount | grep -q " $MOUNT_POINT_REAL "; then
            umount "$MOUNT_POINT" 2>/dev/null || diskutil unmount force "$MOUNT_POINT" 2>/dev/null || true
        fi
        if [[ -n "${PASSTHROUGH_PID:-}" ]]; then
            # SIGTERM, then escalate to SIGKILL after a few seconds if still
            # alive — passthrough's tokio shutdown can hang on macFUSE unmount.
            kill "$PASSTHROUGH_PID" 2>/dev/null || true
            for _ in 1 2 3 4 5 6; do
                kill -0 "$PASSTHROUGH_PID" 2>/dev/null || break
                sleep 0.5
            done
            kill -9 "$PASSTHROUGH_PID" 2>/dev/null || true
            wait "$PASSTHROUGH_PID" 2>/dev/null || true
        fi
    fi
    exit "$rc"
}
trap cleanup EXIT INT TERM

echo "Mounting passthrough: $ROOT_DIR -> $MOUNT_POINT" >&2
"$PASSTHROUGH_BIN" --mountpoint "$MOUNT_POINT" --rootdir "$ROOT_DIR" &
PASSTHROUGH_PID=$!

# Wait up to 10s for the mount to come up.
for _ in $(seq 1 20); do
    if mount | grep -q " $MOUNT_POINT_REAL "; then
        break
    fi
    sleep 0.5
done
if ! mount | grep -q " $MOUNT_POINT_REAL "; then
    echo "FAIL: mount did not come up within 10s" >&2
    exit 1
fi

# --- run --------------------------------------------------------------------

REPORT_DIR="$TARGET_DIR/conformance"
mkdir -p "$REPORT_DIR"
TAP_OUT="$REPORT_DIR/pjdfstest.tap"

echo "Running pjdfstest (output -> $TAP_OUT)" >&2
cd "$MOUNT_POINT"
set +e
"$PROVE" -r --merge --timer "$PJDFSTEST_DIR/tests" >"$TAP_OUT" 2>&1
PROVE_RC=$?
set -e

# `prove` emits a summary, not raw TAP. The trailing line looks like:
#   Files=237, Tests=8677, 315 wallclock secs (...)
# Per-failed-file lines look like:
#   /path/to/foo.t   (Wstat: 0 Tests: 5 Failed: 5)
TOTAL=$(awk -F'[=, ]+' '/^Files=[0-9]+, Tests=[0-9]+/ {for(i=1;i<=NF;i++)if($i=="Tests")print $(i+1)}' "$TAP_OUT" | tail -1)
FAIL_COUNT=$(awk -F'[: ]+' '/Failed: [0-9]+/{for(i=1;i<=NF;i++)if($i=="Failed")sum+=$(i+1)} END{print sum+0}' "$TAP_OUT")
TOTAL=${TOTAL:-0}
PASS_COUNT=$(( TOTAL - FAIL_COUNT ))

echo "pjdfstest: total=$TOTAL pass=$PASS_COUNT fail=$FAIL_COUNT (prove rc=$PROVE_RC)" >&2

# Compare against baseline if one is recorded (skip the TBD placeholder).
BASELINE="$REPO_ROOT/docs/macos-conformance-baseline.md"
if [[ -f "$BASELINE" ]]; then
    EXPECTED_FAIL=$(grep -E '^\| Expected fail count \|' "$BASELINE" \
        | awk -F'|' '{gsub(/ /,"",$3); print $3}' | head -1)
    if [[ -n "${EXPECTED_FAIL:-}" && "$EXPECTED_FAIL" =~ ^[0-9]+$ ]]; then
        if [[ "$FAIL_COUNT" -gt "$EXPECTED_FAIL" ]]; then
            echo "FAIL: $FAIL_COUNT failures > baseline $EXPECTED_FAIL" >&2
            exit 1
        fi
    fi
fi

# `prove` exits non-zero when any test fails; for baseline runs that's the
# expected outcome — exit 0 so the caller can record the numbers without the
# script being treated as failed.
exit 0
