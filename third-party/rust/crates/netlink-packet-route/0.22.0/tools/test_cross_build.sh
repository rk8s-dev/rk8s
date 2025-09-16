#!/bin/bash -ex
# SPDX-License-Identifier: MIT

EXEC_PATH=$(dirname "$(realpath "$0")")
PROJECT_PATH="$(dirname $EXEC_PATH)"

CI_CONFIG_FOR_BUILD="$PROJECT_PATH/.github/workflows/build.yml"
BUILD_TARGETS=$(sed -ne 's/^.*rust_target: "\(.*\)"/\1/p' $CI_CONFIG_FOR_BUILD)

cd $PROJECT_PATH

for BUILD_TARGET in $BUILD_TARGETS;do
    if [ "CHK$(rustup target list --installed \
               | grep $BUILD_TARGET)" == "CHK" ];then
        rustup target add $BUILD_TARGET
    fi

    cargo build --target $BUILD_TARGET
done
