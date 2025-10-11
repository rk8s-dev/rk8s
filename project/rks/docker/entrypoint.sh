#!/bin/sh
set -eu

CONFIG_PATH="${RKS_CONFIG:-/opt/rks/config.yaml}"

if [ ! -f "${CONFIG_PATH}" ]; then
    if [ -f /opt/rks/config.example.yaml ]; then
        echo "[rks] config not found at ${CONFIG_PATH}, falling back to /opt/rks/config.example.yaml" >&2
        CONFIG_PATH=/opt/rks/config.example.yaml
    else
        echo "[rks] config file not found at ${CONFIG_PATH}" >&2
        exit 1
    fi
fi

exec /usr/local/bin/rks start --config "${CONFIG_PATH}" "$@"
