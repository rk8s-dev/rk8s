#!/usr/bin/env bash

set -exo pipefail

rpm -q aardvark-dns aardvark-dns-tests netavark

cd /usr/share/aardvark-dns/
bats test/
