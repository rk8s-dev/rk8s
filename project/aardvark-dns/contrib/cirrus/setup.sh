#!/bin/bash

# This script configures the CI runtime environment.  It's intended
# to be used by Cirrus-CI, not humans.

set -e

source $(dirname $0)/lib.sh

# Only do this once
if [[ -r "/etc/ci_environment" ]]; then
    msg "It appears ${BASH_SOURCE[0]} already ran, exiting."
    exit 0
fi
trap "complete_setup" EXIT

msg "************************************************************"
msg "Setting up runtime environment"
msg "************************************************************"
show_env_vars

req_env_vars NETAVARK_URL NETAVARK_BRANCH
cd /usr/libexec/podman
rm -vf netavark*
if showrun curl --fail --location -o /tmp/netavark.zip "$NETAVARK_URL" && \
   unzip -o /tmp/netavark.zip; then

    if [[ $(uname -m) != "x86_64" ]]; then
        showrun mv netavark.$(uname -m)-unknown-linux-gnu netavark
    fi
    showrun chmod a+x /usr/libexec/podman/netavark
else
    warn "Error downloading/extracting the latest pre-compiled netavark binary from CI"
    showrun cargo install \
      --root /usr/libexec/podman \
      --git https://github.com/containers/netavark \
      --branch "$NETAVARK_BRANCH"
    showrun mv /usr/libexec/podman/bin/netavark /usr/libexec/podman
fi
# show netavark commit in CI logs
showrun /usr/libexec/podman/netavark version

# Warning, this isn't the end.  An exit-handler is installed to finalize
# setup of env. vars.  This is required for runner.sh to operate properly.
# See complete_setup() in lib.sh for details.
