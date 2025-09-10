#!/bin/bash -ex
# SPDX-License-Identifier: MIT

MAIN_BRANCH_NAME="main"
UPSTERAM_GIT="https://github.com/rust-netlink/netlink-packet-route.git"
TMP_CHANGELOG_FILE=$(mktemp)
EDITOR="${EDITOR:-vim}"

if ! command -v jq &> /dev/null
then
    echo "Please install jq to proceed"
    exit 1
fi

if ! command -v cargo set-version &> /dev/null
then
    echo 'Please install cargo-edit via `cargo install cargo-edit` to proceed'
    exit 1
fi


CHANGLOG_FORMAT="
### Breaking changes\n\
 - N/A\n\
\n\
### New features\n\
 - N/A\n\
\n\
### Bug fixes"

function cleanup {
    rm -f $TMP_CHANGELOG_FILE
}

trap cleanup ERR EXIT

CODE_BASE_DIR=$(readlink -f "$(dirname -- "$0")/..");

cd $CODE_BASE_DIR;

CUR_VERSION=$(cargo metadata --no-deps --format-version 1 | \
    jq '.packages[0].version' --raw-output)
CUR_MAJOR_VERSION=$(echo $CUR_VERSION|cut -f1 -d.)
CUR_MINOR_VERSION=$(echo $CUR_VERSION|cut -f2 -d.)
CUR_MICRO_VERSION=$(echo $CUR_VERSION|cut -f3 -d.)

# TODO: Be smart on bumping major, micro or minor version by checking API
#       stability
NEXT_VERSION="${CUR_MAJOR_VERSION}.$((CUR_MINOR_VERSION + 1)).0";

git branch new_release || true
git checkout new_release
git fetch upstream || (git remote add upstream $UPSTERAM_GIT; \
    git fetch upstream)
git reset --hard upstream/$MAIN_BRANCH_NAME

echo "Checking 'cargo publish --dry-run'"
cargo set-version $NEXT_VERSION
cargo publish --dry-run

echo "# Changelog" > $TMP_CHANGELOG_FILE
echo "## [$NEXT_VERSION] - $(date +%F)" >> $TMP_CHANGELOG_FILE
echo -e $CHANGLOG_FORMAT >> $TMP_CHANGELOG_FILE
git log --oneline --format=" - %s. (%h)" \
    v${CUR_VERSION}..upstream/$MAIN_BRANCH_NAME -- | \
    grep -v -E '^ - test:' | \
    grep -v -E '^ - Bump version' | \
    grep -v -E 'cargo clippy'  >> $TMP_CHANGELOG_FILE
echo "" >> $TMP_CHANGELOG_FILE

$EDITOR $TMP_CHANGELOG_FILE
if [ $(wc -l < $TMP_CHANGELOG_FILE) -lt 2 ];then
    echo "No CHANGELOG addition, exiting"
    git checkout CHANGELOG 
    exit 1
fi

CHANGELOG_STR=$(sed -n '3,$p' $TMP_CHANGELOG_FILE|tr '#' '=')
sed -n '2,$p' CHANGELOG >> $TMP_CHANGELOG_FILE

mv $TMP_CHANGELOG_FILE $CODE_BASE_DIR/CHANGELOG
git commit --signoff -a -m "New release ${NEXT_VERSION}" \
    -m "$CHANGELOG_STR"
git push origin +new_release
echo "Please visit github to create pull request for this breach"
