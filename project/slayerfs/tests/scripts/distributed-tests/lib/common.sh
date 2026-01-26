#!/usr/bin/env bash

log_info() {
  printf '[INFO] %s\n' "$*"
}

log_warn() {
  printf '[WARN] %s\n' "$*"
}

log_error() {
  printf '[ERROR] %s\n' "$*" >&2
}

log_success() {
  printf '[OK] %s\n' "$*"
}

die() {
  log_error "$*"
  exit 1
}

require_var() {
  local name="$1"
  if [[ -z "${!name:-}" ]]; then
    die "Missing required config: ${name}"
  fi
}

ensure_dir() {
  local path="$1"
  if [[ -z "$path" ]]; then
    die "ensure_dir called with empty path"
  fi
  mkdir -p "$path"
}

now_stamp() {
  date '+%Y%m%d-%H%M%S'
}
