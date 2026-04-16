#!/usr/bin/env bash

set -euo pipefail

log()  { echo "[$(date '+%H:%M:%S')] $*"; }
info() { log "INFO  $*"; }
ok()   { log "OK    $*"; }
err()  { log "ERROR $*" >&2; }

config_path="${SLAYERFS_CONFIG_PATH:-/run/slayerfs/config.yaml}"
mount_dir="${SLAYERFS_MOUNT_POINT:-/mnt/slayerfs}"
data_backend="${SLAYERFS_DATA_BACKEND:-local-fs}"
data_dir="${SLAYERFS_DATA_DIR:-${SLAYERFS_HOME:-/var/lib/slayerfs}/data}"
meta_backend="${SLAYERFS_META_BACKEND:-redis}"
meta_url="${SLAYERFS_META_URL:-}"
meta_etcd_urls="${SLAYERFS_META_ETCD_URLS:-http://etcd:2379}"
sqlite_path="${SLAYERFS_SQLITE_PATH:-${SLAYERFS_HOME:-/var/lib/slayerfs}/metadata.db}"
log_file="${SLAYERFS_LOG_FILE:-/artifacts/slayerfs.log}"
xfstests_dir="${XFSTESTS_DIR:-/opt/xfstests-dev}"
artifact_root="${SLAYERFS_ARTIFACT_ROOT:-/artifacts}"
artifact_dir="${SLAYERFS_ARTIFACT_DIR:-}"

xfstests_cases="${XFSTESTS_CASES:-}"
xfstests_check_args="${XFSTESTS_CHECK_ARGS:-}"

write_config() {
    mkdir -p "$(dirname "$config_path")" "$mount_dir"
    if [[ "$data_backend" == "local-fs" ]]; then
        mkdir -p "$data_dir"
    fi

    {
        echo "mount_point: $mount_dir"
        echo
        case "$data_backend" in
            local-fs)
                cat <<EOF
data:
  backend: local-fs
  localfs:
    data_dir: ${data_dir}
EOF
                ;;
            s3)
                bucket="${SLAYERFS_S3_BUCKET:-slayerfs-data}"
                region="${SLAYERFS_S3_REGION:-us-east-1}"
                endpoint="${SLAYERFS_S3_ENDPOINT:-http://rustfs:9000}"
                force_path="${SLAYERFS_S3_FORCE_PATH_STYLE:-true}"
                part_size="${SLAYERFS_S3_PART_SIZE:-16777216}"
                max_conc="${SLAYERFS_S3_MAX_CONCURRENCY:-8}"
                cat <<EOF
data:
  backend: s3
  s3:
    bucket: ${bucket}
    region: ${region}
    part_size: ${part_size}
    max_concurrency: ${max_conc}
    force_path_style: ${force_path}
    endpoint: ${endpoint}
EOF
                ;;
            *)
                err "不支持的 SLAYERFS_DATA_BACKEND: $data_backend"
                exit 1
                ;;
        esac
        echo

        case "$meta_backend" in
            sqlite)
                mkdir -p "$(dirname "$sqlite_path")"
                local url="${meta_url:-sqlite://${sqlite_path}?mode=rwc}"
                cat <<EOF
meta:
  backend: sqlx
  sqlx:
    url: "$url"
EOF
                ;;
            redis)
                if [[ -z "$meta_url" ]]; then
                    err "SLAYERFS_META_URL 不能为空 (redis)"
                    exit 1
                fi
                cat <<EOF
meta:
  backend: redis
  redis:
    url: "$meta_url"
EOF
                ;;
            etcd)
                cat <<EOF
meta:
  backend: etcd
  etcd:
    urls:
EOF
                local old_ifs="$IFS"
                IFS=','
                for url in $meta_etcd_urls; do
                    echo "      - \"${url}\""
                done
                IFS="$old_ifs"
                ;;
            *)
                err "不支持的 SLAYERFS_META_BACKEND: $meta_backend"
                exit 1
                ;;
        esac

        echo
        cat <<EOF
layout:
  chunk_size: ${SLAYERFS_CHUNK_SIZE:-67108864}
  block_size: ${SLAYERFS_BLOCK_SIZE:-4194304}
EOF
    } >"$config_path"
}

install_mount_helper() {
    local helper="/usr/sbin/mount.fuse.slayerfs"
    cat >"$helper" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:$PATH"

src="${1:-}"
target="${2:-}"
shift 2 || true

config_path="${SLAYERFS_CONFIG_PATH:-/run/slayerfs/config.yaml}"
log_file="${SLAYERFS_LOG_FILE:-/artifacts/slayerfs.log}"

mkdir -p "$target" "$(dirname "$log_file")"

/usr/local/bin/slayerfs mount --config "$config_path" "$target" >>"$log_file" 2>&1 &
sleep "${SLAYERFS_MOUNT_WAIT_SECS:-1}"
exit 0
EOF
    chmod +x "$helper"
}

write_local_config() {
    cat >"$xfstests_dir/local.config" <<EOF
export TEST_DEV=slayerfs
export TEST_DIR=$mount_dir
export FSTYP=fuse
export FUSE_SUBTYP=.slayerfs
export DF_PROG="df -T -P -a"
EOF
}

prepare_results_dir() {
    mkdir -p "$artifact_dir/results" "$xfstests_dir/results"
    touch "$artifact_dir/results/check.log" "$artifact_dir/check.console.log" >/dev/null 2>&1 || true
}

copy_artifacts() {
    mkdir -p "$artifact_dir"
    if [[ -f "$log_file" && "$log_file" != "$artifact_dir/slayerfs.log" ]]; then
        cp -f "$log_file" "$artifact_dir/slayerfs.log" || true
    fi
    if [[ -f "$config_path" ]]; then
        cp -f "$config_path" "$artifact_dir/backend.yml" || true
    fi
    if [[ -f "$xfstests_dir/local.config" ]]; then
        cp -f "$xfstests_dir/local.config" "$artifact_dir/local.config" || true
    fi
    if [[ -d "$xfstests_dir/results" ]]; then
        mkdir -p "$artifact_dir/results"
        cp -a "$xfstests_dir/results/." "$artifact_dir/results/" 2>/dev/null || true
    fi

    chmod -R a+rwX "$artifact_dir" >/dev/null 2>&1 || true
}

cleanup() {
    while mount | grep -q " on $mount_dir "; do
        fusermount3 -u "$mount_dir" >/dev/null 2>&1 \
            || umount -f "$mount_dir" >/dev/null 2>&1 \
            || umount -l "$mount_dir" >/dev/null 2>&1 \
            || sleep 1
    done
    pkill -f "/usr/local/bin/slayerfs mount" >/dev/null 2>&1 || true
}

on_exit() {
    local status=$?
    copy_artifacts || true
    if [[ -x /usr/local/bin/xfstests_report.sh ]]; then
        bash /usr/local/bin/xfstests_report.sh "$artifact_dir" --no-tar >/dev/null 2>&1 || true
    fi
    cleanup || true
    trap - EXIT
    exit "$status"
}

run_xfstests() {
    local -a check_args=()
    if [[ -n "$xfstests_check_args" ]]; then
        read -r -a check_args <<<"$xfstests_check_args"
    elif [[ -n "$xfstests_cases" ]]; then
        read -r -a check_args <<<"$xfstests_cases"
        check_args=(-fuse -E xfstests_slayer.exclude "${check_args[@]}")
    else
        check_args=(-fuse -E xfstests_slayer.exclude)
    fi

    (
        cd "$xfstests_dir"
        export PATH="$xfstests_dir:$PATH"
        ./check "${check_args[@]}" 2>&1 | tee -a "$artifact_dir/check.console.log" "$artifact_dir/results/check.log"
        exit "${PIPESTATUS[0]}"
    )
}

main() {
    if [[ -z "$artifact_dir" ]]; then
        ts="$(date +%s)-$RANDOM"
        artifact_dir="${artifact_root%/}/run-${ts}"
    fi
    mkdir -p "$artifact_dir"
    chmod a+rwx "$artifact_dir" >/dev/null 2>&1 || true
    log_file="$artifact_dir/slayerfs.log"
    export SLAYERFS_LOG_FILE="$log_file"

    trap on_exit EXIT INT TERM

    info "写入 SlayerFS 配置: $config_path"
    write_config

    info "安装 mount helper: /usr/sbin/mount.fuse.slayerfs"
    install_mount_helper

    info "写入 xfstests local.config: $xfstests_dir/local.config"
    write_local_config

    info "将 xfstests results/ 指向产物目录（便于实时观察 check.log）"
    prepare_results_dir

    info "运行 xfstests (FUSE): dir=$xfstests_dir mount=$mount_dir"
    set +e
    run_xfstests
    status=$?
    set -e

    if [[ -f "$artifact_dir/check.console.log" ]]; then
        cp -f "$artifact_dir/check.console.log" "$artifact_dir/xfstests-script.log" >/dev/null 2>&1 || true
        mkdir -p "$artifact_dir/results"
        cp -f "$artifact_dir/check.console.log" "$artifact_dir/results/check.out" >/dev/null 2>&1 || true
    fi

    copy_artifacts || true
    if [[ -x /usr/local/bin/xfstests_report.sh ]]; then
        bash /usr/local/bin/xfstests_report.sh "$artifact_dir" --no-tar >/dev/null 2>&1 || true
    fi

    if [[ "$status" -eq 0 ]]; then
        ok "xfstests PASS"
    else
        err "xfstests FAIL (exit=$status)"
    fi
    ok "artifacts: $artifact_dir"
    exit "$status"
}

main "$@"
