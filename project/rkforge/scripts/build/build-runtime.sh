#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "${SCRIPT_DIR}/../.." && pwd)"
TOOL="${REPO_ROOT}/tools/build-sandbox-runtime.sh"

if [[ ! -x "${TOOL}" ]]; then
  echo "error: runtime builder not found: ${TOOL}" >&2
  exit 1
fi

if [[ -z "${RKFORGE_LIBKRUN_SRC_DIR:-}" && -d "${REPO_ROOT}/vendor/libkrun" ]]; then
  export RKFORGE_LIBKRUN_SRC_DIR="${REPO_ROOT}/vendor/libkrun"
fi

if [[ -z "${RKFORGE_LIBKRUNFW_SRC_DIR:-}" && -d "${REPO_ROOT}/vendor/libkrunfw" ]]; then
  export RKFORGE_LIBKRUNFW_SRC_DIR="${REPO_ROOT}/vendor/libkrunfw"
fi

if [[ -d "${RKFORGE_LIBKRUN_SRC_DIR:-}" && -d "${RKFORGE_LIBKRUNFW_SRC_DIR:-}" ]]; then
  exec "${TOOL}" --build-from-source "$@"
fi

exec "${TOOL}" "$@"
