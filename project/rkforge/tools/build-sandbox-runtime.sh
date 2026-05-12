#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  build-sandbox-runtime.sh [--output DIR] [--archive TAR] [--rkforge-bin BIN] [--libkrun PATH] [--libkrunfw PATH]
  build-sandbox-runtime.sh --build-from-source [--output DIR] [--archive TAR] [--rkforge-bin BIN] [--libkrun-src DIR] [--libkrunfw-src DIR]

Build a sandbox runtime bundle directory containing:
  - bin/rkforge
  - bin/rkforge-sandbox-shim
  - bin/rkforge-sandbox-agent
  - bin/rkforge-sandbox-guest-init
  - lib/libkrun.so*
  - lib/libkrunfw.so*

Default output:
  ./runtime/current

The produced directory is intended to be discovered automatically by rkforge's
sandbox runtime code during local development, similar to BoxLite's script layer.

When `--archive` is used, real helper binaries must be present in the same target
directory as `rkforge`:

  - rkforge-sandbox-shim
  - rkforge-sandbox-agent
  - rkforge-sandbox-guest-init
EOF
}

OUTPUT_DIR=""
ARCHIVE_PATH=""
RKFORGE_BIN=""
LIBKRUN_PATH=""
LIBKRUNFW_PATH=""
BUILD_FROM_SOURCE=0
LIBKRUN_SRC=""
LIBKRUNFW_SRC=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      OUTPUT_DIR="$2"
      shift 2
      ;;
    --archive)
      ARCHIVE_PATH="$2"
      shift 2
      ;;
    --rkforge-bin)
      RKFORGE_BIN="$2"
      shift 2
      ;;
    --libkrun)
      LIBKRUN_PATH="$2"
      shift 2
      ;;
    --libkrunfw)
      LIBKRUNFW_PATH="$2"
      shift 2
      ;;
    --build-from-source)
      BUILD_FROM_SOURCE=1
      shift
      ;;
    --libkrun-src)
      LIBKRUN_SRC="$2"
      shift 2
      ;;
    --libkrunfw-src)
      LIBKRUNFW_SRC="$2"
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

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/.." && pwd)"

OUTPUT_DIR="${OUTPUT_DIR:-${REPO_ROOT}/runtime/current}"
RKFORGE_BIN="${RKFORGE_BIN:-${REPO_ROOT}/../target/debug/rkforge}"
LIBKRUN_PATH="${LIBKRUN_PATH:-${RKFORGE_LIBKRUN_LIBRARY:-/usr/local/lib64/libkrun.so}}"
LIBKRUNFW_PATH="${LIBKRUNFW_PATH:-${RKFORGE_LIBKRUNFW_PATH:-/usr/local/lib64/libkrunfw.so}}"
LIBKRUN_SRC="${LIBKRUN_SRC:-${RKFORGE_LIBKRUN_SRC_DIR:-${REPO_ROOT}/vendor/libkrun}}"
LIBKRUNFW_SRC="${LIBKRUNFW_SRC:-${RKFORGE_LIBKRUNFW_SRC_DIR:-${REPO_ROOT}/vendor/libkrunfw}}"
RKFORGE_BIN_DIR="$(cd -- "$(dirname -- "${RKFORGE_BIN}")" && pwd)"
SHIM_BIN_CANDIDATE="${RKFORGE_BIN_DIR}/rkforge-sandbox-shim"
AGENT_BIN_CANDIDATE="${RKFORGE_BIN_DIR}/rkforge-sandbox-agent"
GUEST_INIT_BIN_CANDIDATE="${RKFORGE_BIN_DIR}/rkforge-sandbox-guest-init"

die() {
  echo "error: $*" >&2
  exit 1
}

need_file() {
  local path="$1"
  [[ -f "${path}" ]] || die "file not found: ${path}"
}

stage_shared_library() {
  local source="$1"
  local dest_dir="$2"
  local base_name="$3"

  local resolved
  resolved="$(readlink -f "${source}")"
  local actual_name
  actual_name="$(basename "${resolved}")"

  install -D -m 0644 "${resolved}" "${dest_dir}/${actual_name}"
  ln -sfn "${actual_name}" "${dest_dir}/${base_name}"

  local suffix="${actual_name#${base_name}.}"
  if [[ "${suffix}" != "${actual_name}" ]]; then
    local major="${suffix%%.*}"
    ln -sfn "${actual_name}" "${dest_dir}/${base_name}.${major}"
  fi
}

build_libkrunfw_from_source() {
  local source_dir="$1"
  local prefix="$2"
  [[ -f "${source_dir}/Makefile" ]] || die "libkrunfw source tree not found: ${source_dir}"
  make -C "${source_dir}" -j"$(nproc)"
  make -C "${source_dir}" install PREFIX="${prefix}"
}

build_libkrun_from_source() {
  local source_dir="$1"
  local prefix="$2"
  [[ -f "${source_dir}/Makefile" ]] || die "libkrun source tree not found: ${source_dir}"
  make -C "${source_dir}" -j"$(nproc)" BLK=1
  make -C "${source_dir}" install PREFIX="${prefix}" BLK=1
}

find_first_lib() {
  local dir="$1"
  local pattern="$2"
  find "${dir}" -maxdepth 1 -type f -name "${pattern}" | sort | head -n1
}

if [[ "${BUILD_FROM_SOURCE}" == "1" ]]; then
  TMPROOT="$(mktemp -d)"
  trap 'rm -rf "${TMPROOT}"' EXIT

  FW_PREFIX="${TMPROOT}/libkrunfw"
  KRUN_PREFIX="${TMPROOT}/libkrun"
  build_libkrunfw_from_source "${LIBKRUNFW_SRC}" "${FW_PREFIX}"
  build_libkrun_from_source "${LIBKRUN_SRC}" "${KRUN_PREFIX}"

  LIBKRUN_PATH="$(find_first_lib "${KRUN_PREFIX}/lib64" 'libkrun.so*')"
  LIBKRUNFW_PATH="$(find_first_lib "${FW_PREFIX}/lib64" 'libkrunfw.so*')"
fi

need_file "${RKFORGE_BIN}"
need_file "${LIBKRUN_PATH}"
need_file "${LIBKRUNFW_PATH}"

mkdir -p "${OUTPUT_DIR}/bin" "${OUTPUT_DIR}/lib"
install -m 0755 "${RKFORGE_BIN}" "${OUTPUT_DIR}/bin/rkforge"

if [[ -f "${SHIM_BIN_CANDIDATE}" ]]; then
  install -m 0755 "${SHIM_BIN_CANDIDATE}" "${OUTPUT_DIR}/bin/rkforge-sandbox-shim"
else
  [[ -z "${ARCHIVE_PATH}" ]] || die "prebuilt runtime archive requires real helper binary: ${SHIM_BIN_CANDIDATE}"
  ln -sfn rkforge "${OUTPUT_DIR}/bin/rkforge-sandbox-shim"
fi

if [[ -f "${AGENT_BIN_CANDIDATE}" ]]; then
  install -m 0755 "${AGENT_BIN_CANDIDATE}" "${OUTPUT_DIR}/bin/rkforge-sandbox-agent"
else
  [[ -z "${ARCHIVE_PATH}" ]] || die "prebuilt runtime archive requires real helper binary: ${AGENT_BIN_CANDIDATE}"
  ln -sfn rkforge "${OUTPUT_DIR}/bin/rkforge-sandbox-agent"
fi

if [[ -f "${GUEST_INIT_BIN_CANDIDATE}" ]]; then
  install -m 0755 "${GUEST_INIT_BIN_CANDIDATE}" "${OUTPUT_DIR}/bin/rkforge-sandbox-guest-init"
else
  [[ -z "${ARCHIVE_PATH}" ]] || die "prebuilt runtime archive requires real helper binary: ${GUEST_INIT_BIN_CANDIDATE}"
  ln -sfn rkforge "${OUTPUT_DIR}/bin/rkforge-sandbox-guest-init"
fi

stage_shared_library "${LIBKRUN_PATH}" "${OUTPUT_DIR}/lib" "libkrun.so"
stage_shared_library "${LIBKRUNFW_PATH}" "${OUTPUT_DIR}/lib" "libkrunfw.so"

printf 'sandbox runtime bundle created: %s\n' "${OUTPUT_DIR}"

if [[ -n "${ARCHIVE_PATH}" ]]; then
  mkdir -p "$(dirname "${ARCHIVE_PATH}")"
  tar -czf "${ARCHIVE_PATH}" -C "$(dirname "${OUTPUT_DIR}")" "$(basename "${OUTPUT_DIR}")"
  printf 'sandbox runtime archive created: %s\n' "${ARCHIVE_PATH}"
fi
