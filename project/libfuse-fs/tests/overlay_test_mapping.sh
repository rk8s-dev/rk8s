#!/bin/bash

sudo useradd -u 66534 -o -r -d / -s /sbin/nologin host_user || true
sudo groupadd -g 66534 host_group || true

TEST_ROOT="/tmp/overlay_test"
rm -rf "$TEST_ROOT"
mkdir -p "$TEST_ROOT"/{lower,upper,mount}

sudo chown $USER:$USER "$TEST_ROOT/upper"
sudo chmod 777 "$TEST_ROOT/upper"

touch "$TEST_ROOT/lower/owned_by_me_in_lower.txt"

sudo touch "$TEST_ROOT/lower/owned_by_host_user_in_lower.txt"
sudo chown 66534:66534 "$TEST_ROOT/lower/owned_by_host_user_in_lower.txt"

MY_UID=$(id -u)
MY_GID=$(id -g)
UID_MAP="uidmapping=${MY_UID}:0:1:66534:${MY_UID}:1"
GID_MAP="gidmapping=${MY_GID}:0:1:66534:${MY_GID}:1"

sudo ../../target/release/examples/overlayfs_example \
    --mountpoint "$TEST_ROOT/mount" \
    --lowerdir "$TEST_ROOT/lower" \
    --upperdir "$TEST_ROOT/upper" \
    --mapping "${UID_MAP},${GID_MAP}" \
    --privileged \
    --allow-other &

sleep 2
echo "Mount complete."
echo ""

echo "--- Testing reverse mapping (ls in mountpoint) ---"
echo "ls -ln $TEST_ROOT/mount"
ls -ln "$TEST_ROOT/mount"
echo "Expected:"
echo " - owned_by_me_in_lower.txt should be owned by 0 0 (root)"
echo " - owned_by_host_user_in_lower.txt should be owned by 1000 1000 (node)"
echo ""

echo "--- Testing copy-up by modifying a file from lowerdir ---"
sudo -u "#${MY_UID}" touch "$TEST_ROOT/mount/owned_by_host_user_in_lower.txt"
echo "Touched 'owned_by_host_user_in_lower.txt' to trigger copy-up."
echo ""

echo "--- Verifying copy-up result in upperdir ---"
echo "ls -ln $TEST_ROOT/upper"
ls -ln "$TEST_ROOT/upper"
echo "Expected:"
echo " - A new file 'owned_by_host_user_in_lower.txt' should appear in upperdir."
echo " - Crucially, its owner should be 66534:66534 (original host UID/GID)."
echo ""

sudo umount "$TEST_ROOT/mount"
pkill -f overlayfs_example || true
rm -rf "$TEST_ROOT"
sudo userdel host_user || true
sudo groupdel host_group || true