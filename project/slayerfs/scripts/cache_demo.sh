#!/bin/bash
# Simple script to demonstrate SlayerFS cache hits
set -e

CONFIG_FILE="slayerfs-sqlite.yml"
LOG_FILE="cache_hits.log"

echo "=== SlayerFS Cache Hit Demo ==="


echo "Starting SlayerFS..."
cargo run --example persistence_demo -- --config "$CONFIG_FILE" --mount "/tmp/mount" --storage "/tmp/sqlite" > "$LOG_FILE" 2>&1 &
DEMO_PID=$!

# Ensure background process is killed on exit or error
trap 'if kill -0 "$DEMO_PID" 2>/dev/null; then kill "$DEMO_PID"; fi' EXIT ERR INT
# Wait for mount
sleep 3

if ! mountpoint -q "/tmp/mount"; then
    echo "Error: Mount failed"
    exit 1
fi

echo "Filesystem mounted successfully!"

# Create test files
echo "Creating test files..."
for i in {1..3}; do
    echo "Content of file $i - $(date)" > "/tmp/mount/test$i.txt"
done

echo "Reading files multiple times to trigger cache hits..."
for round in {1..3}; do
    echo "Round $round:"
    for i in {1..3}; do
        cat "/tmp/mount/test$i.txt" > /dev/null
    done
done

echo "Checking logs for cache messages..."
echo "=== Cache-related messages ==="
grep -iE "cache|hit|remove|chunks" "$LOG_FILE" | head -20

echo ""
echo "Demo completed. Full log saved to: $LOG_FILE"
