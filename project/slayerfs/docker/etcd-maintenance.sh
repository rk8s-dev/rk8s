#!/bin/sh

set -eu

retention="${ETCD_COMPACT_RETENTION_REVISIONS:-5000}"
interval_secs="${ETCD_MAINTENANCE_INTERVAL_SECS:-30}"

while true; do
  status="$(etcdctl endpoint status -w json 2>/dev/null || true)"
  revision="$(printf '%s' "$status" | sed -n 's/.*"revision"[[:space:]]*:[[:space:]]*\([0-9][0-9]*\).*/\1/p' | head -n 1)"

  if [ -n "$revision" ] && [ "$revision" -gt "$retention" ]; then
    compact_to=$((revision - retention))
    etcdctl compact "$compact_to" >/dev/null 2>&1 || true
    etcdctl defrag >/dev/null 2>&1 || true
    etcdctl alarm disarm >/dev/null 2>&1 || true
  fi

  sleep "$interval_secs"
done