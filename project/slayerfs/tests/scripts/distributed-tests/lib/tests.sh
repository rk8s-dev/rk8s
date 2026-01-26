#!/usr/bin/env bash

remote_results_root() {
  local fs_name="$1"
  printf '%s/%s/%s' "${REMOTE_RESULTS_DIR}" "${RUN_ID}" "$fs_name"
}

run_with_progress() {
  local node="$1"
  local label="$2"
  local cmd="$3"
  local interval="${TEST_PROGRESS_INTERVAL:-10}"
  local start
  start=$(date +%s)

  log_info "Start ${label}"
  ssh_run "$node" "$cmd" &
  local pid=$!

  while kill -0 "$pid" 2>/dev/null; do
    sleep "$interval"
    if kill -0 "$pid" 2>/dev/null; then
      local now elapsed
      now=$(date +%s)
      elapsed=$((now - start))
      log_info "  ${label} running... ${elapsed}s elapsed"
    fi
  done

  wait "$pid"
  local status=$?
  local end elapsed
  end=$(date +%s)
  elapsed=$((end - start))
  if [[ $status -eq 0 ]]; then
    log_success "Done ${label} in ${elapsed}s"
  else
    log_error "Failed ${label} after ${elapsed}s (exit ${status})"
    return "$status"
  fi
}

run_fio() {
  local fs_name="$1"
  local mount_dir="$2"

  if ! ssh_run "$PRIMARY_CLIENT" "command -v fio >/dev/null 2>&1"; then
    log_warn "fio not found on ${PRIMARY_CLIENT}; skipping"
    return 0
  fi

  local out_dir
  out_dir="$(remote_results_root "$fs_name")/fio"

  ssh_run "$PRIMARY_CLIENT" "mkdir -p '${out_dir}'"

  local common="--ioengine=libaio --direct=${FIO_DIRECT} --numjobs=${FIO_NUMJOBS} --iodepth=${FIO_IODEPTH} --time_based --runtime=${FIO_RUNTIME} --group_reporting"

  run_with_progress "$PRIMARY_CLIENT" "fio (1/4) seqwrite" "sudo fio --name=seqwrite --directory='${mount_dir}' --rw=write --bs='${FIO_BS_SEQ}' --size='${FIO_SIZE}' ${common} --output-format=json --output='${out_dir}/seqwrite.json'"
  run_with_progress "$PRIMARY_CLIENT" "fio (2/4) seqread" "sudo fio --name=seqwrite --directory='${mount_dir}' --rw=read --bs='${FIO_BS_SEQ}' ${common} --output-format=json --output='${out_dir}/seqread.json'"
  run_with_progress "$PRIMARY_CLIENT" "fio (3/4) randwrite" "sudo fio --name=randwrite --directory='${mount_dir}' --rw=randwrite --bs='${FIO_BS_RAND}' --size='${FIO_SIZE}' ${common} --output-format=json --output='${out_dir}/randwrite.json'"
  run_with_progress "$PRIMARY_CLIENT" "fio (4/4) randread" "sudo fio --name=randwrite --directory='${mount_dir}' --rw=randread --bs='${FIO_BS_RAND}' ${common} --output-format=json --output='${out_dir}/randread.json'"
  }

run_mdtest() {
  local fs_name="$1"
  local mount_dir="$2"

  if ! ssh_run "$PRIMARY_CLIENT" "command -v mdtest >/dev/null 2>&1"; then
    log_warn "mdtest not found on ${PRIMARY_CLIENT}; skipping"
    return 0
  fi

  local out_dir
  out_dir="$(remote_results_root "$fs_name")/mdtest"

  ssh_run "$PRIMARY_CLIENT" "mkdir -p '${out_dir}'"

  local mdtest_dir="${mount_dir}/mdtest-${RUN_ID}"
  local cmd
  if [[ -n "${MDTEST_ARGS:-}" ]]; then
    cmd="mdtest ${MDTEST_ARGS} -d '${mdtest_dir}'"
  else
    cmd="mdtest -d '${mdtest_dir}' -n ${MDTEST_NFILES} -u -t -F -C -T -r -W -e -p ${MDTEST_NPROCS}"
  fi

  run_with_progress "$PRIMARY_CLIENT" "mdtest" "sudo ${cmd} | tee '${out_dir}/mdtest.log'"
  ssh_run "$PRIMARY_CLIENT" "sudo rm -rf '${mdtest_dir}'"
}

run_iozone() {
  local fs_name="$1"
  local mount_dir="$2"

  if ! ssh_run "$PRIMARY_CLIENT" "command -v iozone >/dev/null 2>&1"; then
    log_error "iozone not found. Please install iozone manually or set RUN_IOZONE=0"
    die "Required dependency missing: iozone"
  fi

  local out_dir
  out_dir="$(remote_results_root "$fs_name")/iozone"

  ssh_run "$PRIMARY_CLIENT" "mkdir -p '${out_dir}'"

  local iozone_dir="${mount_dir}/iozone-${RUN_ID}"
  local iozone_size="${IOZONE_SIZE:-10G}"
  local iozone_file="${iozone_dir}/iozone.file"

  ssh_exec_sudo "$PRIMARY_CLIENT" "mkdir -p '${iozone_dir}'"

  # IOzone test parameters
  local cmd
  if [[ -n "${IOZONE_ARGS:-}" ]]; then
    cmd="sudo iozone ${IOZONE_ARGS} -f ${iozone_file} 2>&1 | tee '${out_dir}/iozone.log'"
  else
    # Default: automatic mode with 10G file, 1M record size, 4K to 1M mix
    cmd="sudo iozone -a -A -r 4k -i 1m -s ${iozone_size} -f ${iozone_file} 2>&1 | tee '${out_dir}/iozone.log'"
  fi

  run_with_progress "$PRIMARY_CLIENT" "iozone" "${cmd}"
  ssh_exec_sudo "$PRIMARY_CLIENT" "rm -rf '${iozone_dir}'"
}

run_xfstests() {
  local fs_name="$1"
  local mount_dir="$2"

  if [[ -z "${XFSTESTS_DIR:-}" ]]; then
    log_warn "XFSTESTS_DIR not set; skipping"
    return 0
  fi

  # Auto-install xfstests if not found
  if ! ssh_run "$PRIMARY_CLIENT" "test -d '${XFSTESTS_DIR}'"; then
    log_info "xfstests not found, installing on ${PRIMARY_CLIENT}..."

    log_info "  Installing xfstests dependencies..."
    ssh_exec_sudo "$PRIMARY_CLIENT" "apt-get update && apt-get install -y acl attr automake bc dbench dump e2fsprogs fio gawk gcc git libacl1-dev libaio-dev libcap-dev libgdbm-dev libtool libtool-bin liburing-dev libuuid1 lvm2 make psmisc python3 quota sed uuid-dev uuid-runtime xfsprogs linux-headers-$(uname -r) sqlite3 fuse3" || return 1

    log_info "  Cloning xfstests..."
    ssh_exec_sudo "$PRIMARY_CLIENT" "rm -rf /tmp/xfstests-dev && git clone -b v2023.12.10 git://git.kernel.org/pub/scm/fs/xfs/xfstests-dev.git /tmp/xfstests-dev" || return 1

    log_info "  Building xfstests..."
    ssh_exec_sudo "$PRIMARY_CLIENT" "cd /tmp/xfstests-dev && make && sudo make install" || return 1

    # Create XFSTESTS_DIR if custom path was specified
    if [[ "${XFSTESTS_DIR}" != "/tmp/xfstests-dev" ]]; then
      ssh_exec_sudo "$PRIMARY_CLIENT" "mkdir -p '${XFSTESTS_DIR}'"
    fi

    log_success "xfstests installed"
  fi

  local out_dir
  out_dir="$(remote_results_root "$fs_name")/xfstests"
  ssh_run "$PRIMARY_CLIENT" "mkdir -p '${out_dir}'"

  # Copy exclude file if exists
  local exclude_file="${SCRIPT_DIR}/../xfstests_slayer.exclude"
  if [[ -f "$exclude_file" ]]; then
    log_info "  Copying xfstests exclude file..."
    scp_to "$PRIMARY_CLIENT" "$exclude_file" "/tmp/xfstests_slayer.exclude"
  fi

  if [[ -n "${XFSTESTS_CMD:-}" ]]; then
    run_with_progress "$PRIMARY_CLIENT" "xfstests" "cd '${XFSTESTS_DIR}' && ${XFSTESTS_CMD} | tee '${out_dir}/xfstests.log'"
    return 0
  fi

  local fuse_subtyp="${XFSTESTS_FUSE_SUBTYP:-.slayerfs}"
  local cmd
  cmd="cd '${XFSTESTS_DIR}' && TEST_DIR='${mount_dir}' TEST_DEV='${fs_name}' FSTYP=fuse FUSE_SUBTYP='${fuse_subtyp}' DF_PROG='df -T -P -a' ./check ${XFSTESTS_ARGS}"

  if [[ -f "$exclude_file" ]]; then
    cmd+=" -E xfstests_slayer.exclude"
  fi

  run_with_progress "$PRIMARY_CLIENT" "xfstests" "${cmd} | tee '${out_dir}/xfstests.log'"
}

run_tests_for_fs() {
  local fs_name="$1"
  local mount_dir="$2"

  if [[ "${RUN_FIO:-0}" == "1" ]]; then
    run_fio "$fs_name" "$mount_dir"
  fi

  if [[ "${RUN_MDTEST:-0}" == "1" ]]; then
    run_mdtest "$fs_name" "$mount_dir"
  fi

  if [[ "${RUN_IOZONE:-0}" == "1" ]]; then
    run_iozone "$fs_name" "$mount_dir"
  fi

  if [[ "${RUN_XFSTESTS:-0}" == "1" ]]; then
    run_xfstests "$fs_name" "$mount_dir"
  fi

}
