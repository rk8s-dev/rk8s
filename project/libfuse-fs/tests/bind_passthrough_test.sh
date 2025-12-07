#!/bin/bash

set -e

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo -e "${GREEN}=== Testing PassthroughFS with Bind Mounts ===${NC}"

# Check if running as root
if [ "$EUID" -ne 0 ]; then 
    echo -e "${RED}ERROR: This test requires root privileges${NC}"
    exit 1
fi

# Setup test directories
TEST_DIR="/tmp/passthrough_bind_test_$$"
ROOT_DIR="$TEST_DIR/root"
MOUNT_POINT="$TEST_DIR/merged"

cleanup() {
    echo -e "${YELLOW}Cleaning up...${NC}"
    
    # Try to unmount bind mounts
    for mount in "$MOUNT_POINT/proc" "$MOUNT_POINT/sys" "$MOUNT_POINT/dev/pts" "$MOUNT_POINT/dev"; do
        if mountpoint -q "$mount" 2>/dev/null; then
            echo "Unmounting $mount"
            umount -l "$mount" 2>/dev/null || true
        fi
    done
    
    # Unmount the passthrough filesystem
    if mountpoint -q "$MOUNT_POINT" 2>/dev/null; then
        echo "Unmounting $MOUNT_POINT"
        fusermount -u "$MOUNT_POINT" 2>/dev/null || umount -l "$MOUNT_POINT" 2>/dev/null || true
        sleep 1
    fi
    
    # Remove test directories
    rm -rf "$TEST_DIR"
    echo -e "${GREEN}Cleanup completed${NC}"
}

trap cleanup EXIT INT TERM

# Create test directories
mkdir -p "$ROOT_DIR" "$MOUNT_POINT"
# Don't create subdirectories - they will be created by bind mount manager

# Create some test files
echo "test content" > "$ROOT_DIR/test.txt"
mkdir -p "$ROOT_DIR/testdir"
echo "test file" > "$ROOT_DIR/testdir/file.txt"

# Find the binary
BINARY="${CARGO_TARGET_DIR:-../target}/debug/examples/passthrough"
if [ ! -f "$BINARY" ]; then
    BINARY="target/debug/examples/passthrough"
fi

if [ ! -f "$BINARY" ]; then
    echo -e "${RED}ERROR: Cannot find passthrough binary${NC}"
    echo "Expected at: $BINARY"
    exit 1
fi

echo -e "${GREEN}Found binary: $BINARY${NC}"

# Start the filesystem with bind mounts in background
echo -e "${YELLOW}Starting PassthroughFS with bind mounts...${NC}"
"$BINARY" \
    --mountpoint "$MOUNT_POINT" \
    --rootdir "$ROOT_DIR" \
    --bind "proc:/proc" \
    --bind "sys:/sys" \
    --bind "dev:/dev" \
    --bind "dev/pts:/dev/pts" \
    --privileged \
    --allow-other &

FS_PID=$!
echo "Filesystem PID: $FS_PID"

# Wait for mount to be ready
sleep 2

# Check if process is still running
if ! kill -0 $FS_PID 2>/dev/null; then
    echo -e "${RED}ERROR: Filesystem process died${NC}"
    wait $FS_PID
    exit 1
fi

# Verify mount point is mounted
if ! mountpoint -q "$MOUNT_POINT"; then
    echo -e "${RED}ERROR: Mount point not mounted${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

echo -e "${GREEN}✓ Filesystem mounted${NC}"

# Test 1: Verify bind mounts are present
echo -e "${YELLOW}Test 1: Checking bind mounts...${NC}"

if mountpoint -q "$MOUNT_POINT/proc"; then
    echo -e "${GREEN}✓ /proc is bind mounted${NC}"
else
    echo -e "${RED}✗ /proc is NOT bind mounted${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

if mountpoint -q "$MOUNT_POINT/sys"; then
    echo -e "${GREEN}✓ /sys is bind mounted${NC}"
else
    echo -e "${RED}✗ /sys is NOT bind mounted${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

if mountpoint -q "$MOUNT_POINT/dev"; then
    echo -e "${GREEN}✓ /dev is bind mounted${NC}"
else
    echo -e "${RED}✗ /dev is NOT bind mounted${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

# Test 2: Verify we can read from bind mounts
echo -e "${YELLOW}Test 2: Reading from bind mounts...${NC}"

if [ -f "$MOUNT_POINT/proc/version" ]; then
    PROC_VERSION=$(cat "$MOUNT_POINT/proc/version")
    echo -e "${GREEN}✓ Can read /proc/version: ${PROC_VERSION:0:50}...${NC}"
else
    echo -e "${RED}✗ Cannot read /proc/version${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

# Test 3: Verify passthrough functionality still works
echo -e "${YELLOW}Test 3: Testing passthrough functionality...${NC}"

if [ -f "$MOUNT_POINT/test.txt" ]; then
    CONTENT=$(cat "$MOUNT_POINT/test.txt")
    if [ "$CONTENT" = "test content" ]; then
        echo -e "${GREEN}✓ Can read root directory files${NC}"
    else
        echo -e "${RED}✗ Content mismatch${NC}"
        kill $FS_PID 2>/dev/null || true
        exit 1
    fi
else
    echo -e "${RED}✗ Cannot see root directory files${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

# Write to a file
echo "new content" > "$MOUNT_POINT/newfile.txt"
if [ -f "$ROOT_DIR/newfile.txt" ]; then
    echo -e "${GREEN}✓ Writes work correctly${NC}"
else
    echo -e "${RED}✗ Writes not working${NC}"
    kill $FS_PID 2>/dev/null || true
    exit 1
fi

# Test 4: Test graceful shutdown
echo -e "${YELLOW}Test 4: Testing graceful shutdown...${NC}"
kill -TERM $FS_PID
wait $FS_PID 2>/dev/null || true
sleep 1

# Verify bind mounts were unmounted
UNMOUNTED=true
for mount in "$MOUNT_POINT/proc" "$MOUNT_POINT/sys" "$MOUNT_POINT/dev"; do
    if mountpoint -q "$mount" 2>/dev/null; then
        echo -e "${RED}✗ $mount still mounted after shutdown${NC}"
        UNMOUNTED=false
    fi
done

if [ "$UNMOUNTED" = true ]; then
    echo -e "${GREEN}✓ All bind mounts cleaned up${NC}"
else
    echo -e "${RED}✗ Some bind mounts not cleaned up${NC}"
    exit 1
fi

echo -e "${GREEN}=== All tests passed! ===${NC}"
