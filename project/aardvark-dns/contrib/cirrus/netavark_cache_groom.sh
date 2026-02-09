#!/bin/bash
#
# This script is intended to be run from Cirrus-CI to prepare the
# rust targets cache for re-use during subsequent runs.  This mainly
# involves removing files and directories which change frequently
# but are cheap/quick to regenerate - i.e. prevent "cache-flapping".
# Any other use of this script is not supported and may cause harm.

set -eo pipefail

SCRIPT_DIRPATH=$(dirname ${BASH_SOURCE[0]})
source $SCRIPT_DIRPATH/lib.sh

if [[ "$CIRRUS_CI" != true ]] || [[ -z "$NETAVARK_BRANCH" ]]; then
  die "Script is not intended for use outside of Cirrus-CI"
fi

SCRIPT_DEST=$SCRIPT_DIRPATH/cache_groom.sh
showrun curl --location --silent --show-error -o $SCRIPT_DEST \
  https://raw.githubusercontent.com/containers/netavark/$NETAVARK_BRANCH/contrib/cirrus/cache_groom.sh

# Certain common automation library calls assume execution from this file
exec bash $SCRIPT_DEST
