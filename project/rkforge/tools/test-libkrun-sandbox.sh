#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'EOF'
Usage:
  test-libkrun-sandbox.sh

Environment overrides:
  DOCKER_IMAGE      Base image to export as guest rootfs.
                    Default: python:3.12-slim
  SANDBOX_ID        Sandbox name to create.
                    Default: demo-<timestamp>
  PYTHON_CODE       Python snippet to run inside the guest.
                    Default: print('hello from guest')
  RKFORGE_BIN       Explicit rkforge binary path.
  RKFORGE_LIBKRUN_LIBRARY
                    Default: /usr/local/lib64/libkrun.so
  RKFORGE_LIBKRUNFW_PATH
                    Default: /usr/local/lib64/libkrunfw.so
  RKFORGE_SANDBOX_INHERIT_STDIO
                    Default: 1
  RUST_LOG          Default: rkforge=debug
  KEEP_SANDBOX      Set to 1 to skip stop/rm on exit.
  KEEP_ARTIFACTS    Set to 1 to keep exported rootfs tar and ext4 image.

This script:
  1. Builds rkforge if needed
  2. Exports a Docker rootfs from DOCKER_IMAGE
  3. Installs libseccomp2 into that rootfs image before export
  4. Builds a guest ext4 image
  5. Runs sandbox create/exec/inspect/stop/rm against libkrun
EOF
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "${SCRIPT_DIR}/.." && pwd)"

DOCKER_IMAGE="${DOCKER_IMAGE:-python:3.12-slim}"
SANDBOX_ID="${SANDBOX_ID:-demo-$(date +%s)}"
PYTHON_CODE="${PYTHON_CODE:-print('hello from guest')}"
RKFORGE_LIBKRUN_LIBRARY="${RKFORGE_LIBKRUN_LIBRARY:-/usr/local/lib64/libkrun.so}"
RKFORGE_LIBKRUNFW_PATH="${RKFORGE_LIBKRUNFW_PATH:-/usr/local/lib64/libkrunfw.so}"
RKFORGE_SANDBOX_INHERIT_STDIO="${RKFORGE_SANDBOX_INHERIT_STDIO:-1}"
RUST_LOG="${RUST_LOG:-rkforge=debug}"
KEEP_SANDBOX="${KEEP_SANDBOX:-0}"
KEEP_ARTIFACTS="${KEEP_ARTIFACTS:-0}"

TMPDIR="$(mktemp -d)"
ROOTFS_TAR="${TMPDIR}/guest-rootfs.tar"
GUEST_IMAGE="${TMPDIR}/rkforge-sandbox.ext4"
DOCKER_CID=""
SANDBOX_CREATED=0

log() {
  printf '==> %s\n' "$*"
}

die() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

need_cmd() {
  command -v "$1" >/dev/null 2>&1 || die "required command not found: $1"
}

find_rkforge_bin() {
  local candidate

  if [[ -n "${RKFORGE_BIN:-}" ]]; then
    [[ -x "${RKFORGE_BIN}" ]] || die "RKFORGE_BIN is not executable: ${RKFORGE_BIN}"
    printf '%s\n' "${RKFORGE_BIN}"
    return 0
  fi

  for candidate in \
    "${REPO_ROOT}/target/debug/rkforge" \
    "${REPO_ROOT}/../target/debug/rkforge"
  do
    if [[ -x "${candidate}" ]]; then
      printf '%s\n' "${candidate}"
      return 0
    fi
  done

  if command -v rkforge >/dev/null 2>&1; then
    command -v rkforge
    return 0
  fi

  return 1
}

cleanup() {
  set +e

  if [[ -n "${DOCKER_CID}" ]]; then
    docker rm -f "${DOCKER_CID}" >/dev/null 2>&1 || true
  fi

  if [[ "${KEEP_SANDBOX}" != "1" && "${SANDBOX_CREATED}" == "1" && -x "${RKFORGE_BIN_PATH:-}" ]]; then
    "${RKFORGE_BIN_PATH}" sandbox stop "${SANDBOX_ID}" >/dev/null 2>&1 || true
    "${RKFORGE_BIN_PATH}" sandbox rm "${SANDBOX_ID}" --force >/dev/null 2>&1 || true
  fi

  if [[ "${KEEP_ARTIFACTS}" != "1" ]]; then
    rm -rf "${TMPDIR}"
  fi
}

dump_debug() {
  local store_root instance_dir

  store_root="${XDG_DATA_HOME:-${HOME}/.local/share}/rkforge/sandbox"
  instance_dir="${store_root}/instances/${SANDBOX_ID}"

  printf '\n'
  log "sandbox debug info"
  "${RKFORGE_BIN_PATH}" sandbox inspect "${SANDBOX_ID}" || true

  if [[ -d "${instance_dir}" ]]; then
    log "instance dir: ${instance_dir}"
    ls -al "${instance_dir}" || true
    for file in shim.log runtime-state.json shim-state.json guest-ready.json vm-spec.json vm-handle.json; do
      if [[ -f "${instance_dir}/${file}" ]]; then
        printf '\n--- %s ---\n' "${instance_dir}/${file}"
        cat "${instance_dir}/${file}" || true
      fi
    done
  fi

  if [[ "${KEEP_ARTIFACTS}" == "1" ]]; then
    log "artifacts kept in ${TMPDIR}"
  fi
}

trap cleanup EXIT

need_cmd cargo
need_cmd docker
need_cmd mkfs.ext4

if [[ ! -e /dev/kvm || ! -r /dev/kvm || ! -w /dev/kvm ]]; then
  die "/dev/kvm is not accessible to the current user"
fi

[[ -f "${RKFORGE_LIBKRUN_LIBRARY}" ]] || die "libkrun not found: ${RKFORGE_LIBKRUN_LIBRARY}"
[[ -f "${RKFORGE_LIBKRUNFW_PATH}" ]] || die "libkrunfw not found: ${RKFORGE_LIBKRUNFW_PATH}"

if ! RKFORGE_BIN_PATH="$(find_rkforge_bin)"; then
  log "rkforge binary not found, building it"
  (cd "${REPO_ROOT}" && cargo build -p rkforge)
  RKFORGE_BIN_PATH="$(find_rkforge_bin)" || die "failed to locate rkforge after build"
fi

log "building rkforge to ensure the latest local changes are included"
(cd "${REPO_ROOT}" && cargo build -p rkforge)
RKFORGE_BIN_PATH="$(find_rkforge_bin)" || die "failed to locate rkforge after build"

log "using rkforge binary: ${RKFORGE_BIN_PATH}"
# log "pulling docker image: ${DOCKER_IMAGE}"
# docker pull "${DOCKER_IMAGE}"

log "creating temporary rootfs container"
DOCKER_CID="$(docker run -d --entrypoint sh "${DOCKER_IMAGE}" -lc 'sleep infinity')"

log "installing libseccomp2 into guest rootfs"
docker exec "${DOCKER_CID}" sh -lc \
  'apt-get update && DEBIAN_FRONTEND=noninteractive apt-get install -y --no-install-recommends libseccomp2 && rm -rf /var/lib/apt/lists/*'

log "exporting rootfs tar: ${ROOTFS_TAR}"
docker export "${DOCKER_CID}" -o "${ROOTFS_TAR}"
docker rm -f "${DOCKER_CID}" >/dev/null
DOCKER_CID=""

log "building guest image: ${GUEST_IMAGE}"
"${REPO_ROOT}/tools/build-sandbox-guest-rootfs.sh" \
  --rootfs-tar "${ROOTFS_TAR}" \
  --output "${GUEST_IMAGE}" \
  --rkforge-bin "${RKFORGE_BIN_PATH}"

export RKFORGE_LIBKRUN_LIBRARY
export RKFORGE_LIBKRUNFW_PATH
export RKFORGE_SANDBOX_VMM=libkrun
export RKFORGE_SANDBOX_GUEST_IMAGE="${GUEST_IMAGE}"
export RKFORGE_SANDBOX_INHERIT_STDIO
export RUST_LOG

log "sandbox env"
printf '  RKFORGE_LIBKRUN_LIBRARY=%s\n' "${RKFORGE_LIBKRUN_LIBRARY}"
printf '  RKFORGE_LIBKRUNFW_PATH=%s\n' "${RKFORGE_LIBKRUNFW_PATH}"
printf '  RKFORGE_SANDBOX_GUEST_IMAGE=%s\n' "${RKFORGE_SANDBOX_GUEST_IMAGE}"
printf '  RKFORGE_SANDBOX_INHERIT_STDIO=%s\n' "${RKFORGE_SANDBOX_INHERIT_STDIO}"
printf '  RUST_LOG=%s\n' "${RUST_LOG}"
printf '  SANDBOX_ID=%s\n' "${SANDBOX_ID}"

log "removing any stale sandbox with the same id"
"${RKFORGE_BIN_PATH}" sandbox rm "${SANDBOX_ID}" --force >/dev/null 2>&1 || true

log "creating sandbox"
"${RKFORGE_BIN_PATH}" sandbox create --name "${SANDBOX_ID}"
SANDBOX_CREATED=1

log "executing test python inside guest"
set +e
"${RKFORGE_BIN_PATH}" sandbox exec "${SANDBOX_ID}" --code "${PYTHON_CODE}"
rc=$?
set -e
if [[ "${rc}" -ne 0 ]]; then
  dump_debug
  exit "${rc}"
fi

log "final sandbox inspect"
"${RKFORGE_BIN_PATH}" sandbox inspect "${SANDBOX_ID}"

if [[ "${KEEP_SANDBOX}" == "1" ]]; then
  log "sandbox kept for inspection: ${SANDBOX_ID}"
else
  log "stopping sandbox"
  "${RKFORGE_BIN_PATH}" sandbox stop "${SANDBOX_ID}"
  log "removing sandbox"
  "${RKFORGE_BIN_PATH}" sandbox rm "${SANDBOX_ID}" --force
  SANDBOX_CREATED=0
fi

if [[ "${KEEP_ARTIFACTS}" == "1" ]]; then
  log "artifacts kept in ${TMPDIR}"
fi

log "sandbox libkrun test completed"
