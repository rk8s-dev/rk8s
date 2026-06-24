#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  build-sandbox-guest-rootfs.sh --rootfs-dir DIR --output IMG [--rkforge-bin BIN]
  build-sandbox-guest-rootfs.sh --rootfs-tar TAR --output IMG [--rkforge-bin BIN]

Build an ext4 guest image suitable for libkrun by injecting rkforge at:
  - /usr/local/bin/rkforge

The resulting guest rootfs can be used with libkrun's built-in init plus
`krun_set_exec(...)` to launch `rkforge sandbox-agent` inside the guest.

Requirements:
  - mkfs.ext4 with `-d`
  - a Linux rootfs that already contains Python 3
EOF
}

ROOTFS_DIR=""
ROOTFS_TAR=""
OUTPUT=""
RKFORGE_BIN="${RKFORGE_BIN:-target/debug/rkforge}"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --rootfs-dir)
      ROOTFS_DIR="$2"
      shift 2
      ;;
    --rootfs-tar)
      ROOTFS_TAR="$2"
      shift 2
      ;;
    --output)
      OUTPUT="$2"
      shift 2
      ;;
    --rkforge-bin)
      RKFORGE_BIN="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$OUTPUT" ]]; then
  echo "--output is required" >&2
  exit 1
fi

if [[ -n "$ROOTFS_DIR" && -n "$ROOTFS_TAR" ]]; then
  echo "choose only one of --rootfs-dir or --rootfs-tar" >&2
  exit 1
fi

if [[ -z "$ROOTFS_DIR" && -z "$ROOTFS_TAR" ]]; then
  echo "one of --rootfs-dir or --rootfs-tar is required" >&2
  exit 1
fi

if [[ ! -x "$RKFORGE_BIN" ]]; then
  echo "rkforge binary not found or not executable: $RKFORGE_BIN" >&2
  exit 1
fi

if ! command -v mkfs.ext4 >/dev/null 2>&1; then
  echo "mkfs.ext4 is required" >&2
  exit 1
fi

workdir="$(mktemp -d)"
cleanup() {
  rm -rf "$workdir"
}
trap cleanup EXIT

staging="$workdir/rootfs"
mkdir -p "$staging"

if [[ -n "$ROOTFS_TAR" ]]; then
  tar -xf "$ROOTFS_TAR" -C "$staging"
else
  cp -a "$ROOTFS_DIR"/. "$staging"/
fi

mkdir -p "$staging/usr/local/bin" "$staging/proc" "$staging/sys" "$staging/dev" "$staging/run" "$staging/tmp"
install -m 0755 "$RKFORGE_BIN" "$staging/usr/local/bin/rkforge"

if [[ ! -x "$staging/usr/bin/python3" && ! -x "$staging/bin/python3" && ! -x "$staging/usr/local/bin/python3" ]]; then
  echo "warning: python3 was not found in the guest rootfs; sandbox exec_python will fail until Python is installed" >&2
fi

size_mb="$(du -sm "$staging" | awk '{print $1}')"
size_mb="$(( size_mb + 256 ))"

mkdir -p "$(dirname "$OUTPUT")"
rm -f "$OUTPUT"
truncate -s 0 "$OUTPUT"
mkfs.ext4 -q -d "$staging" -F "$OUTPUT" "${size_mb}M"

echo "guest image created: $OUTPUT"
