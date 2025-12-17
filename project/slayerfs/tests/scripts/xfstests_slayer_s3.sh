#!/bin/bash

set -euo pipefail

current_dir=$(dirname "$(realpath "$0")")
workspace_dir=$(realpath "$current_dir/../../..")
redis_config="$workspace_dir/slayerfs/slayerfs-sqlite.yml"
backend_dir=/tmp/data
mount_dir=/tmp/mount
log_file=/tmp/slayerfs.log
persistence_bin="$workspace_dir/target/release/examples/persistence_s3_demo"

if [[ -z "$persistence_bin" ]]; then
    echo "Cannot find slayerfs persistence_s3_demo binary."
    echo "Please run: cargo build -p slayerfs --example persistence_s3_demo --release"
    exit 1
fi

sudo rm -rf "$backend_dir"
while mount | grep -q "$mount_dir"; do
    sudo umount -f "$mount_dir" || sleep 1
done
sudo rm -rf "$mount_dir"
sudo rm -rf /tmp/xfstests-dev
sudo mkdir -p "$backend_dir" "$mount_dir"
sudo rm -f "$log_file"

sudo apt-get update
sudo apt-get install acl attr automake bc dbench dump e2fsprogs fio gawk \
    gcc git indent libacl1-dev libaio-dev libcap-dev libgdbm-dev libtool \
    libtool-bin liburing-dev libuuid1 lvm2 make psmisc python3 quota sed \
    uuid-dev uuid-runtime xfsprogs linux-headers-$(uname -r) sqlite3 \
    fuse3
sudo apt-get install exfatprogs f2fs-tools ocfs2-tools udftools xfsdump \
    xfslibs-dev

# clone xfstests and install.
cd /tmp/
git clone -b v2023.12.10 git://git.kernel.org/pub/scm/fs/xfs/xfstests-dev.git
cd xfstests-dev
make
sudo make install

# overwrite local config.
cat >local.config  <<EOF
export TEST_DEV=slayerfs
export TEST_DIR=$mount_dir
export FSTYP=fuse
export FUSE_SUBTYP=.slayerfs

#Deleting the following command will result in an error: TEST_DEV=slayerfs is mounted but not a type fuse filesystem.
export DF_PROG="df -T -P -a"
EOF

# create fuse mount script for slayerfs.
sudo cat >/usr/sbin/mount.fuse.slayerfs  <<EOF
#!/bin/bash
set -euo pipefail

export PATH="/usr/local/sbin:/usr/local/bin:/usr/sbin:/usr/bin:/sbin:/bin:\$PATH"

# S3 environment variables
export S3_BUCKET="\${S3_BUCKET:-Bucketname}"
export S3_ENDPOINT="\${S3_ENDPOINT:-http://endpoint}"
export AWS_ACCESS_KEY_ID="\${AWS_ACCESS_KEY_ID:-YourID}"
export AWS_SECRET_ACCESS_KEY="\${AWS_SECRET_ACCESS_KEY:-YourKey}"
export AWS_REGION="\${AWS_REGION:-us-east-1}"

ulimit -n 1048576
CONFIG_PATH="$redis_config"
LOG_FILE="$log_file"
PERSISTENCE_BIN="$persistence_bin"

META_DIR="$backend_dir"
MOUNT_DIR="$mount_dir"

"\$PERSISTENCE_BIN" \
  -c "\$CONFIG_PATH" \
  -m "\$MOUNT_DIR" \
  --meta-dir "\$META_DIR" \
  --bucket "\$S3_BUCKET" \
  --endpoint "\$S3_ENDPOINT" >>"\$LOG_FILE" 2>&1 &
sleep 1
EOF
sudo chmod +x /usr/sbin/mount.fuse.slayerfs

echo "====> Start to run xfstests."
# Copy exclude list
sudo cp "$current_dir/xfstests_slayer.exclude" /tmp/xfstests-dev/

# run tests.
cd /tmp/xfstests-dev
sudo LC_ALL=C ./check -fuse generic/001