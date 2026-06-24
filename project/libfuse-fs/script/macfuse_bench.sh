#!/usr/bin/env bash
# macFUSE performance baseline for libfuse-fs passthrough.
#
# Mounts examples/passthrough on macFUSE, runs the fio jobs in
# bench/fio.cfg, parses results into JSON, and appends to the baseline doc.
#
# Skips cleanly on non-macOS or when macFUSE / fio aren't available.
#
# Modes
# -----
#   default        single run (lazy mode, label = $LABEL or timestamp)
#   --ab           lazy=true vs lazy=false back-to-back for AB_RUNS rounds,
#                  prints per-job median ratio, writes raw runs plus a JSON
#                  summary artifact, and fails if gates are not met.
#                  Aborts if any other macFUSE mount is detected (system
#                  noise dominated past results).
#
# Optional env:
#   RUN_MACFUSE_BENCH  Must be 1; otherwise the script skips.
#   MOUNT_POINT         Default: /tmp/libfusefs-bench-mnt
#   ROOT_DIR            Default: /tmp/libfusefs-bench-root
#   PASSTHROUGH_BIN     Default: target/release/examples/passthrough
#   FIO_CFG             Default: bench/fio.cfg
#   LABEL               Free-form label tagging this run (e.g. "baseline-pre-A")
#   KEEP_MOUNT          Set to 1 to leave mount up afterwards
#   AB_ALLOW_NOISE      Set to 1 to bypass the no-other-mount precondition
#                       (use only when you accept noisy numbers).
#   AB_RUNS             Number of lazy/eager pairs in --ab mode. Default: 3
#   AB_META_MIN_RATIO   Required meta-stat lazy/eager median ratio. Default: 2.0
#   AB_IO_MAX_REGRESSION  Max allowed IO regression. Default: 0.10

set -euo pipefail

AB_MODE=0
if [[ "${1:-}" == "--ab" ]]; then
    AB_MODE=1
    shift
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"

# Resolve the cargo target directory used by this workspace. Honour
# CARGO_TARGET_DIR; otherwise probe the package's own target/ first, then walk
# up looking for a workspace-level target/.
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

skip() { echo "SKIP: $*" >&2; exit 0; }

# --- env probes -------------------------------------------------------------

[[ "$(uname -s)" == "Darwin" ]] || skip "not on macOS"
[[ "${RUN_MACFUSE_BENCH:-0}" == "1" ]] || skip "RUN_MACFUSE_BENCH!=1"
MACFUSE_BIN="/Library/Filesystems/macfuse.fs/Contents/Resources/mount_macfuse"
[[ -x "$MACFUSE_BIN" ]] || skip "macFUSE not installed"
command -v fio >/dev/null 2>&1 || skip "fio not installed (try: brew install fio)"

PASSTHROUGH_BIN="${PASSTHROUGH_BIN:-$TARGET_DIR/release/examples/passthrough}"
if [[ ! -x "$PASSTHROUGH_BIN" ]]; then
    echo "Building passthrough example (release)…" >&2
    (cd "$REPO_ROOT" && cargo build --release --example passthrough)
fi

FIO_CFG="${FIO_CFG:-$REPO_ROOT/bench/fio.cfg}"
[[ -f "$FIO_CFG" ]] || { echo "FAIL: fio config $FIO_CFG missing" >&2; exit 1; }

MOUNT_POINT="${MOUNT_POINT:-/tmp/libfusefs-bench-mnt}"
ROOT_DIR="${ROOT_DIR:-/tmp/libfusefs-bench-root}"
LABEL="${LABEL:-$(date -u +%Y%m%dT%H%M%SZ)}"
AB_RUNS="${AB_RUNS:-3}"
AB_META_MIN_RATIO="${AB_META_MIN_RATIO:-2.0}"
AB_IO_MAX_REGRESSION="${AB_IO_MAX_REGRESSION:-0.10}"

[[ "$AB_RUNS" =~ ^[0-9]+$ ]] && [[ "$AB_RUNS" -gt 0 ]] \
    || { echo "FAIL: AB_RUNS must be a positive integer" >&2; exit 1; }

mkdir -p "$MOUNT_POINT" "$ROOT_DIR"
# `mount(8)` reports realpaths (`/tmp` resolves to `/private/tmp` on macOS),
# so we compare against the canonicalized form when checking readiness.
MOUNT_POINT_REAL="$(cd "$MOUNT_POINT" && pwd -P)"

# Number of macFUSE mounts present *outside* this script's mountpoint.
# Past A/B runs were dominated by system noise from concurrent mounts on
# the dev machine; a non-zero count is recorded in the baseline so future
# readers can discount affected numbers.
#
# Implemented in pure awk (instead of `grep -vF | wc -l`) so an empty
# match doesn't trip `set -e -o pipefail`.
count_competing_macfuse_mounts() {
    mount | awk -v self="$MOUNT_POINT_REAL" '/macfuse/ && $3 != self {n++} END {print n+0}'
}

CONCURRENT_MOUNTS="$(count_competing_macfuse_mounts)"

# --- cache flush helper -----------------------------------------------------

flush_caches() {
    sync
    if command -v purge >/dev/null 2>&1; then
        sudo -n purge 2>/dev/null || purge 2>/dev/null || true
    fi
}

REPORT_DIR="$TARGET_DIR/bench"
mkdir -p "$REPORT_DIR"

# --- single-run helper ------------------------------------------------------
#
# Mounts the passthrough example, runs fio, writes raw JSON to
# $REPORT_DIR/<run_label>.json, and appends a markdown table to the
# baseline doc. Optional first arg: extra flags for the example.
#
# Sets: RUN_JSON (path to fio JSON for caller's analysis).
run_once() {
    local run_label="$1"; shift
    local extra_args=("$@")

    local pass_pid=""
    cleanup_run() {
        if [[ -n "$pass_pid" ]]; then
            if mount | grep -q " $MOUNT_POINT_REAL "; then
                umount "$MOUNT_POINT" 2>/dev/null \
                    || diskutil unmount force "$MOUNT_POINT" 2>/dev/null || true
            fi
            kill "$pass_pid" 2>/dev/null || true
            for _ in 1 2 3 4 5 6; do
                kill -0 "$pass_pid" 2>/dev/null || break
                sleep 0.5
            done
            kill -9 "$pass_pid" 2>/dev/null || true
            wait "$pass_pid" 2>/dev/null || true
        fi
    }
    trap cleanup_run RETURN

    # Clean any stale state in the backing dir from prior runs.
    rm -rf "$ROOT_DIR/bench-data" 2>/dev/null || true

    echo "[$run_label] Mounting: $ROOT_DIR -> $MOUNT_POINT (${extra_args[*]:-default})" >&2
    # Splice optional extra args without tripping `set -u` on an empty array.
    "$PASSTHROUGH_BIN" --mountpoint "$MOUNT_POINT" --rootdir "$ROOT_DIR" \
        ${extra_args[@]+"${extra_args[@]}"} &
    pass_pid=$!

    for _ in $(seq 1 20); do
        mount | grep -q " $MOUNT_POINT_REAL " && break
        sleep 0.5
    done
    mount | grep -q " $MOUNT_POINT_REAL " \
        || { echo "FAIL[$run_label]: mount not up" >&2; return 1; }

    local data_dir="$MOUNT_POINT/bench-data"
    mkdir -p "$data_dir"

    flush_caches
    RUN_JSON="$REPORT_DIR/macfuse-bench-$run_label.json"
    echo "[$run_label] Running fio: $FIO_CFG" >&2
    ( cd "$data_dir" && fio --output-format=json "$FIO_CFG" ) >"$RUN_JSON"

    REPO_ROOT="$REPO_ROOT" CONCURRENT_MOUNTS="$CONCURRENT_MOUNTS" \
        python3 - "$RUN_JSON" "$run_label" <<'PYEOF'
import json, os, sys, datetime, pathlib

raw = json.loads(pathlib.Path(sys.argv[1]).read_text())
label = sys.argv[2]
concurrent = os.environ.get("CONCURRENT_MOUNTS", "?")

rows = []
for job in raw.get("jobs", []):
    name = job.get("jobname", "?")
    read = job.get("read", {})
    iops = read.get("iops", 0.0)
    p50  = read.get("clat_ns", {}).get("percentile", {}).get("50.000000", 0)
    p99  = read.get("clat_ns", {}).get("percentile", {}).get("99.000000", 0)
    rows.append((name, iops, p50, p99))

print(f"=== macFUSE bench summary [{label}] ===")
print(f"{'job':<16} {'iops':>12} {'p50_us':>10} {'p99_us':>10}")
for name, iops, p50, p99 in rows:
    print(f"{name:<16} {iops:>12.1f} {p50/1000:>10.1f} {p99/1000:>10.1f}")

repo = pathlib.Path(os.environ.get("REPO_ROOT", "."))
doc = repo / "docs" / "macos-performance-baseline.md"
if doc.exists():
    block = [
        "",
        f"### Run `{label}` — {datetime.datetime.utcnow().isoformat()}Z (concurrent_mounts={concurrent})",
        "",
        "| job | iops | p50 (µs) | p99 (µs) |",
        "| --- | ---: | ---: | ---: |",
    ]
    for name, iops, p50, p99 in rows:
        block.append(f"| {name} | {iops:.1f} | {p50/1000:.1f} | {p99/1000:.1f} |")
    with doc.open("a") as f:
        f.write("\n".join(block) + "\n")
    print(f"Appended summary to {doc}")
PYEOF
}

# --- mode dispatch ----------------------------------------------------------

if [[ "$AB_MODE" == "1" ]]; then
    if [[ "$CONCURRENT_MOUNTS" -gt 0 && "${AB_ALLOW_NOISE:-0}" != "1" ]]; then
        echo "FAIL: $CONCURRENT_MOUNTS other macFUSE mount(s) detected." >&2
        echo "      A/B numbers are dominated by competing-mount noise on this" >&2
        echo "      box (see docs/macos-performance-baseline.md). Unmount them" >&2
        echo "      or set AB_ALLOW_NOISE=1 to override." >&2
        mount | awk '/macfuse/' | sed 's/^/    /' >&2
        exit 1
    fi
    lazy_jsons=()
    eager_jsons=()
    for run_id in $(seq 1 "$AB_RUNS"); do
        LAZY_LABEL="${LABEL}-r${run_id}-lazy"
        EAGER_LABEL="${LABEL}-r${run_id}-eager"

        run_once "$LAZY_LABEL"
        lazy_jsons+=("$REPORT_DIR/macfuse-bench-$LAZY_LABEL.json")

        run_once "$EAGER_LABEL" --macos-eager
        eager_jsons+=("$REPORT_DIR/macfuse-bench-$EAGER_LABEL.json")
    done

    SUMMARY_JSON="$REPORT_DIR/macfuse-bench-$LABEL-ab-summary.json"
    REPO_ROOT="$REPO_ROOT" CONCURRENT_MOUNTS="$CONCURRENT_MOUNTS" \
    AB_RUNS="$AB_RUNS" AB_META_MIN_RATIO="$AB_META_MIN_RATIO" \
    AB_IO_MAX_REGRESSION="$AB_IO_MAX_REGRESSION" SUMMARY_JSON="$SUMMARY_JSON" \
        python3 - "$LABEL" "${lazy_jsons[@]}" "::" "${eager_jsons[@]}" <<'PYEOF'
import datetime
import json
import os
import pathlib
import statistics
import sys

label = sys.argv[1]
sep = sys.argv.index("::")
lazy_paths = [pathlib.Path(p) for p in sys.argv[2:sep]]
eager_paths = [pathlib.Path(p) for p in sys.argv[sep + 1:]]
concurrent = int(os.environ.get("CONCURRENT_MOUNTS", "0"))
ab_runs = int(os.environ.get("AB_RUNS", "3"))
meta_min_ratio = float(os.environ.get("AB_META_MIN_RATIO", "2.0"))
io_max_regression = float(os.environ.get("AB_IO_MAX_REGRESSION", "0.10"))
summary_path = pathlib.Path(os.environ["SUMMARY_JSON"])

def load_iops(path):
    raw = json.loads(path.read_text())
    return {
        job.get("jobname", "?"): float(job.get("read", {}).get("iops", 0.0))
        for job in raw.get("jobs", [])
    }

lazy_runs = [load_iops(p) for p in lazy_paths]
eager_runs = [load_iops(p) for p in eager_paths]
jobs = sorted(set().union(*(r.keys() for r in lazy_runs), *(r.keys() for r in eager_runs)))

rows = []
failures = []
for job in jobs:
    lazy_vals = [r.get(job, 0.0) for r in lazy_runs]
    eager_vals = [r.get(job, 0.0) for r in eager_runs]
    lazy_median = statistics.median(lazy_vals)
    eager_median = statistics.median(eager_vals)
    ratio = lazy_median / eager_median if eager_median > 0 else None
    min_ratio = None
    if job == "meta-stat":
        min_ratio = meta_min_ratio
    elif job in ("randread-4k", "seqread-1m"):
        min_ratio = 1.0 - io_max_regression
    status = "ok"
    if ratio is None:
        status = "fail"
        failures.append(f"{job}: eager median iops is zero")
    elif min_ratio is not None and ratio < min_ratio:
        status = "fail"
        failures.append(f"{job}: ratio {ratio:.2f}x < required {min_ratio:.2f}x")
    rows.append({
        "job": job,
        "lazy_iops_median": lazy_median,
        "eager_iops_median": eager_median,
        "ratio": ratio,
        "required_min_ratio": min_ratio,
        "status": status,
    })

for required in ("meta-stat", "randread-4k", "seqread-1m"):
    if required not in jobs:
        failures.append(f"{required}: missing from fio output")

summary = {
    "label": label,
    "timestamp_utc": datetime.datetime.utcnow().isoformat() + "Z",
    "ab_runs": ab_runs,
    "concurrent_mounts": concurrent,
    "thresholds": {
        "meta_stat_min_ratio": meta_min_ratio,
        "io_min_ratio": 1.0 - io_max_regression,
        "io_max_regression": io_max_regression,
    },
    "raw": {
        "lazy": [str(p) for p in lazy_paths],
        "eager": [str(p) for p in eager_paths],
    },
    "jobs": rows,
    "failures": failures,
}
summary_path.write_text(json.dumps(summary, indent=2, sort_keys=True) + "\n")

print()
print(f"=== A/B median ratio (lazy / eager) [{label}] runs={ab_runs} concurrent_mounts={concurrent} ===")
print(f"{'job':<16} {'lazy_med_iops':>14} {'eager_med_iops':>15} {'ratio':>8} {'gate':>10}")
for row in rows:
    req = row["required_min_ratio"]
    gate = "-" if req is None else f">={req:.2f}x"
    ratio = row["ratio"]
    ratio_text = "n/a" if ratio is None else f"{ratio:.2f}x"
    print(
        f"{row['job']:<16} {row['lazy_iops_median']:>14.1f} "
        f"{row['eager_iops_median']:>15.1f} {ratio_text:>9} {gate:>10}"
    )
print(f"Summary JSON: {summary_path}")

repo = pathlib.Path(os.environ.get("REPO_ROOT", "."))
doc = repo / "docs" / "macos-performance-baseline.md"
if doc.exists():
    block = [
        "",
        f"### A/B median `{label}` - {summary['timestamp_utc']} (runs={ab_runs}, concurrent_mounts={concurrent})",
        "",
        "| job | lazy median iops | eager median iops | ratio | gate | status |",
        "| --- | ---: | ---: | ---: | ---: | --- |",
    ]
    for row in rows:
        req = row["required_min_ratio"]
        gate = "-" if req is None else f">= {req:.2f}x"
        ratio = row["ratio"]
        ratio_text = "n/a" if ratio is None else f"{ratio:.2f}x"
        block.append(
            f"| {row['job']} | {row['lazy_iops_median']:.1f} | "
            f"{row['eager_iops_median']:.1f} | {ratio_text} | "
            f"{gate} | {row['status']} |"
        )
    block.append(f"\nSummary JSON: `{summary_path}`")
    with doc.open("a") as f:
        f.write("\n".join(block) + "\n")
    print(f"Appended A/B median summary to {doc}")

if failures:
    print("A/B gates failed:", file=sys.stderr)
    for failure in failures:
        print(f"  - {failure}", file=sys.stderr)
    sys.exit(2)
PYEOF
    echo "Done. Summary: $SUMMARY_JSON" >&2
else
    run_once "$LABEL"
    echo "Raw fio output: $REPORT_DIR/macfuse-bench-$LABEL.json" >&2
fi
