#!/usr/bin/env bash
# macFUSE passthrough smoke test for libfuse-fs.
#
# This is intentionally opt-in. It needs a working macFUSE installation and a
# runner where the kernel extension/system extension can be loaded.
#
# Optional env:
#   MACFUSE_SMOKE_REQUIRED=0  Skip instead of fail when the mount does not
#                             come up. Use only for advisory GitHub-hosted CI.

set -euo pipefail

skip() {
    echo "SKIP: $*" >&2
    exit 0
}

fail() {
    echo "FAIL: $*" >&2
    exit 1
}

smoke_required() {
    [[ "${MACFUSE_SMOKE_REQUIRED:-1}" != "0" ]]
}

skip_or_fail() {
    if smoke_required; then
        fail "$*"
    else
        skip "$*"
    fi
}

[[ "$(uname -s)" == "Darwin" ]] || skip "not on macOS"
[[ "${RUN_MACFUSE_TESTS:-0}" == "1" ]] || skip "RUN_MACFUSE_TESTS!=1"

MACFUSE_BIN="/Library/Filesystems/macfuse.fs/Contents/Resources/mount_macfuse"
[[ -x "$MACFUSE_BIN" ]] || skip_or_fail "macFUSE not installed"

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

resolve_target_dir() {
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        echo "$CARGO_TARGET_DIR"
        return
    fi
    cargo metadata --no-deps --format-version 1 \
        --manifest-path "$REPO_ROOT/Cargo.toml" \
        | python3 -c 'import json, sys; print(json.load(sys.stdin)["target_directory"])'
}

TARGET_DIR="$(resolve_target_dir)"
PASSTHROUGH_BIN_FROM_ENV="${PASSTHROUGH_BIN:-}"
PASSTHROUGH_BIN="${PASSTHROUGH_BIN:-$TARGET_DIR/debug/examples/passthrough}"
if [[ ! -x "$PASSTHROUGH_BIN" ]]; then
    [[ -z "$PASSTHROUGH_BIN_FROM_ENV" ]] || fail "PASSTHROUGH_BIN is not executable: $PASSTHROUGH_BIN"
    echo "Building passthrough example..." >&2
    (cd "$REPO_ROOT" && cargo build --example passthrough)
fi

SMOKE_LOOPS="${SMOKE_LOOPS:-1}"
[[ "$SMOKE_LOOPS" =~ ^[0-9]+$ ]] && [[ "$SMOKE_LOOPS" -gt 0 ]] \
    || fail "SMOKE_LOOPS must be a positive integer"

TMP_PARENT=""
if [[ -z "${ROOT_DIR:-}" || -z "${MOUNT_POINT:-}" ]]; then
    TMP_PARENT="$(mktemp -d "${TMPDIR:-/tmp}/libfusefs-smoke.XXXXXX")"
fi
ROOT_DIR="${ROOT_DIR:-$TMP_PARENT/root}"
MOUNT_POINT="${MOUNT_POINT:-$TMP_PARENT/mnt}"

PASSTHROUGH_PID=""
MOUNT_POINT_REAL=""

safe_rm_dir() {
    local p="$1"
    case "$p" in
        ""|"/"|"/private"|"/tmp"|"/private/tmp")
            fail "refusing to remove unsafe path: $p"
            ;;
    esac
    rm -rf "$p"
}

is_mounted() {
    [[ -n "$MOUNT_POINT_REAL" ]] && mount | grep -q " $MOUNT_POINT_REAL "
}

stop_passthrough() {
    if [[ -n "$PASSTHROUGH_PID" ]]; then
        kill "$PASSTHROUGH_PID" 2>/dev/null || true
        for _ in 1 2 3 4 5 6; do
            kill -0 "$PASSTHROUGH_PID" 2>/dev/null || break
            sleep 0.5
        done
        kill -9 "$PASSTHROUGH_PID" 2>/dev/null || true
        wait "$PASSTHROUGH_PID" 2>/dev/null || true
        PASSTHROUGH_PID=""
    fi
}

unmount_smoke() {
    if is_mounted; then
        umount "$MOUNT_POINT" 2>/dev/null \
            || diskutil unmount force "$MOUNT_POINT" 2>/dev/null \
            || true
    fi
}

cleanup() {
    local status=$?
    unmount_smoke
    stop_passthrough
    if [[ "${KEEP_MOUNT:-0}" != "1" ]]; then
        [[ -n "${ROOT_DIR:-}" ]] && safe_rm_dir "$ROOT_DIR"
        [[ -n "${MOUNT_POINT:-}" ]] && safe_rm_dir "$MOUNT_POINT"
        [[ -n "$TMP_PARENT" ]] && safe_rm_dir "$TMP_PARENT"
    fi
    exit "$status"
}
trap cleanup EXIT

reset_dirs() {
    unmount_smoke
    stop_passthrough
    if [[ "${KEEP_MOUNT:-0}" != "1" ]]; then
        safe_rm_dir "$ROOT_DIR"
        safe_rm_dir "$MOUNT_POINT"
    fi
    mkdir -p "$ROOT_DIR" "$MOUNT_POINT"
    MOUNT_POINT_REAL="$(cd "$MOUNT_POINT" && pwd -P)"
}

wait_for_mount() {
    for _ in $(seq 1 30); do
        is_mounted && return 0
        if [[ -n "$PASSTHROUGH_PID" ]] && ! kill -0 "$PASSTHROUGH_PID" 2>/dev/null; then
            return 1
        fi
        sleep 0.5
    done
    return 1
}

run_exchange_check() {
    python3 - "$MOUNT_POINT/exchange-a.txt" "$MOUNT_POINT/exchange-b.txt" <<'PYEOF'
import ctypes
import os
import sys

a, b = sys.argv[1], sys.argv[2]
with open(a, "w", encoding="utf-8") as f:
    f.write("left")
with open(b, "w", encoding="utf-8") as f:
    f.write("right")

libc = ctypes.CDLL("/usr/lib/libSystem.B.dylib", use_errno=True)
exchangedata = libc.exchangedata
exchangedata.argtypes = [ctypes.c_char_p, ctypes.c_char_p, ctypes.c_uint]
exchangedata.restype = ctypes.c_int
rc = exchangedata(os.fsencode(a), os.fsencode(b), 0)
if rc != 0:
    err = ctypes.get_errno()
    raise OSError(err, os.strerror(err))

if open(a, encoding="utf-8").read() != "right":
    raise AssertionError("exchange-a did not receive right")
if open(b, encoding="utf-8").read() != "left":
    raise AssertionError("exchange-b did not receive left")
PYEOF
}

run_one_loop() {
    local loop_id="$1"
    reset_dirs
    echo "hello-from-smoke" > "$ROOT_DIR/marker.txt"

    echo "[$loop_id/$SMOKE_LOOPS] Mounting: $ROOT_DIR -> $MOUNT_POINT" >&2
    "$PASSTHROUGH_BIN" --mountpoint "$MOUNT_POINT" --rootdir "$ROOT_DIR" &
    PASSTHROUGH_PID=$!

    if ! wait_for_mount; then
        skip_or_fail "mount did not come up; macFUSE is installed but not loadable on this runner"
    fi

    test "$(cat "$MOUNT_POINT/marker.txt")" = "hello-from-smoke"
    echo "smoke-write" > "$MOUNT_POINT/written.txt"
    test "$(cat "$ROOT_DIR/written.txt")" = "smoke-write"

    /usr/bin/xattr -w user.libfuse-smoke ok "$MOUNT_POINT/written.txt"
    test "$(/usr/bin/xattr -p user.libfuse-smoke "$MOUNT_POINT/written.txt")" = "ok"

    echo "rename" > "$MOUNT_POINT/rename-src.txt"
    mv -n "$MOUNT_POINT/rename-src.txt" "$MOUNT_POINT/rename-dst.txt"
    test -f "$ROOT_DIR/rename-dst.txt"
    test ! -e "$ROOT_DIR/rename-src.txt"

    run_exchange_check

    local birthtime
    birthtime="$(stat -f %B "$MOUNT_POINT/written.txt")"
    [[ "$birthtime" =~ ^[0-9]+$ ]] && [[ "$birthtime" -gt 0 ]] \
        || fail "invalid birthtime from stat: $birthtime"

    unmount_smoke
    stop_passthrough
    is_mounted && fail "mount still present after unmount"
}

for i in $(seq 1 "$SMOKE_LOOPS"); do
    run_one_loop "$i"
done

echo "macFUSE smoke OK ($SMOKE_LOOPS loop(s))"
