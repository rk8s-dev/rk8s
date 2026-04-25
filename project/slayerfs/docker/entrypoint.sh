#!/usr/bin/env bash

set -euo pipefail

write_data_section() {
  local backend="${SLAYERFS_DATA_BACKEND:-local-fs}"

  case "$backend" in
    local-fs)
      cat <<EOF
data:
  backend: local-fs
  localfs:
    data_dir: ${SLAYERFS_DATA_DIR:-${SLAYERFS_HOME:-/var/lib/slayerfs}/data}
EOF
      ;;
    s3)
      : "${SLAYERFS_S3_BUCKET:?SLAYERFS_S3_BUCKET is required when SLAYERFS_DATA_BACKEND=s3}"
      cat <<EOF
data:
  backend: s3
  s3:
    bucket: ${SLAYERFS_S3_BUCKET}
    region: ${SLAYERFS_S3_REGION:-us-east-1}
    part_size: ${SLAYERFS_S3_PART_SIZE:-16777216}
    max_concurrency: ${SLAYERFS_S3_MAX_CONCURRENCY:-8}
    force_path_style: ${SLAYERFS_S3_FORCE_PATH_STYLE:-false}
EOF
      if [[ -n "${SLAYERFS_S3_ENDPOINT:-}" ]]; then
        echo "    endpoint: ${SLAYERFS_S3_ENDPOINT}"
      fi
      ;;
    *)
      echo "unsupported SLAYERFS_DATA_BACKEND: $backend" >&2
      exit 1
      ;;
  esac
}

write_meta_section() {
  local home_dir="${SLAYERFS_HOME:-/var/lib/slayerfs}"
  local backend="${SLAYERFS_META_BACKEND:-sqlite}"
  local sqlite_path="${SLAYERFS_SQLITE_PATH:-${home_dir}/metadata.db}"
  local meta_url="${SLAYERFS_META_URL:-}"

  case "$backend" in
    sqlite)
      mkdir -p "$(dirname "$sqlite_path")"
      if [[ -z "$meta_url" ]]; then
        meta_url="sqlite://${sqlite_path}?mode=rwc"
      fi
      cat <<EOF
meta:
  backend: sqlx
  sqlx:
    url: "$meta_url"
EOF
      ;;
    sqlx|postgres)
      : "${meta_url:?SLAYERFS_META_URL is required when SLAYERFS_META_BACKEND=${backend}}"
      cat <<EOF
meta:
  backend: sqlx
  sqlx:
    url: "$meta_url"
EOF
      ;;
    redis)
      : "${meta_url:?SLAYERFS_META_URL is required when SLAYERFS_META_BACKEND=redis}"
      cat <<EOF
meta:
  backend: redis
  redis:
    url: "$meta_url"
EOF
      ;;
    etcd)
      local etcd_urls="${SLAYERFS_META_ETCD_URLS:-http://etcd:2379}"
      cat <<EOF
meta:
  backend: etcd
  etcd:
    urls:
EOF
      local old_ifs="$IFS"
      IFS=','
      for url in $etcd_urls; do
        echo "      - \"${url}\""
      done
      IFS="$old_ifs"
      ;;
    *)
      echo "unsupported SLAYERFS_META_BACKEND: $backend" >&2
      exit 1
      ;;
  esac
}

write_config() {
  local mount_point="${SLAYERFS_MOUNT_POINT:-/mnt/slayerfs}"
  local config_path="${SLAYERFS_CONFIG_PATH:-/run/slayerfs/config.yaml}"

  mkdir -p "$(dirname "$config_path")" "$mount_point"
  if [[ "${SLAYERFS_DATA_BACKEND:-local-fs}" == "local-fs" ]]; then
    mkdir -p "${SLAYERFS_DATA_DIR:-${SLAYERFS_HOME:-/var/lib/slayerfs}/data}"
  fi

  {
    echo "mount_point: $mount_point"
    echo
    write_data_section
    echo
    write_meta_section
    echo
    cat <<EOF
layout:
  chunk_size: ${SLAYERFS_CHUNK_SIZE:-67108864}
  block_size: ${SLAYERFS_BLOCK_SIZE:-4194304}
EOF
  } >"$config_path"
}

prepare_fuse() {
  mkdir -p /etc
  if ! grep -q '^user_allow_other$' /etc/fuse.conf 2>/dev/null; then
    echo 'user_allow_other' >> /etc/fuse.conf
  fi
}

main() {
    if [[ $# -gt 0 ]]; then
        exec "$@"
    fi

  prepare_fuse
  write_config

  exec /usr/local/bin/slayerfs mount \
    --config "${SLAYERFS_CONFIG_PATH:-/run/slayerfs/config.yaml}" \
    "${SLAYERFS_MOUNT_POINT:-/mnt/slayerfs}"
}

main "$@"