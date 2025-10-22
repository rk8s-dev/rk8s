#!/bin/bash

sudo useradd -u 66534 -o -r -d / -s /sbin/nologin host_user || true
sudo groupadd -g 66534 host_group || true

TEST_ROOT="/tmp/passthrough_test"
rm -rf "$TEST_ROOT"
mkdir -p "$TEST_ROOT"/{source,mount}

sudo chown $USER:$USER "$TEST_ROOT/source"
sudo chmod 777 "$TEST_ROOT/source"

touch "$TEST_ROOT/source/owned_by_me.txt"
sudo touch "$TEST_ROOT/source/owned_by_host_user.txt"
sudo chown 66534:66534 "$TEST_ROOT/source/owned_by_host_user.txt"

MY_UID=$(id -u)
MY_GID=$(id -g)
UID_MAP="uidmapping=${MY_UID}:0:1:66534:${MY_UID}:1"
GID_MAP="gidmapping=${MY_GID}:0:1:66534:${MY_GID}:1"

sudo ../../target/release/examples/passthrough_example \
    --mountpoint "$TEST_ROOT/mount" \
    --rootdir "$TEST_ROOT/source" \
    -o "${UID_MAP},${GID_MAP}" \
    --allow-other &

sleep 2
echo "Mount complete."
echo ""

echo "--- Testing reverse mapping (ls in mountpoint) ---"
echo "ls -ln $TEST_ROOT/mount"
ls -ln "$TEST_ROOT/mount"
echo "Expected:"
echo " - owned_by_me.txt should be owned by 0 0 (container root)"
echo " - owned_by_host_user.txt should be owned by 1000 1000 (container node)"
echo ""

echo "--- Testing forward mapping (creating new files) ---"
sudo -u "#${MY_UID}" touch "$TEST_ROOT/mount/created_by_container_node.txt"
sudo touch "$TEST_ROOT/mount/created_by_container_root.txt"
echo "ls -ln $TEST_ROOT/mount"
ls -ln "$TEST_ROOT/mount"
echo "Expected in source directory:"
echo " - created_by_container_node.txt should be owned by ${MY_UID}:${MY_GID}"
echo " - created_by_container_root.txt should be owned by 0:0"
echo ""
echo "ls -ln $TEST_ROOT/source"
ls -ln "$TEST_ROOT/source"
echo "Expected in source directory:"
echo " - created_by_container_node.txt should be owned by 66534:66534"
echo " - created_by_container_root.txt should be owned by ${MY_UID}:${MY_GID}"
echo ""

sudo umount "$TEST_ROOT/mount"
pkill -f passthrough_example || true
rm -rf "$TEST_ROOT"
sudo userdel host_user || true
sudo groupdel host_group || true