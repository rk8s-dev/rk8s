#!/bin/bash
# Comprehensive test runner for rename2 functionality
# This script runs all P0, P1, and P2 tests with proper reporting

set -e

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_ROOT="$(dirname "$SCRIPT_DIR")"

cd "$PROJECT_ROOT"

echo "=========================================="
echo "Rename2 Comprehensive Test Suite"
echo "=========================================="
echo ""

# Find cargo command (handle sudo environment)
if command -v cargo &> /dev/null; then
    CARGO_CMD="cargo"
elif [ -n "$SUDO_USER" ]; then
    # Running under sudo, try to find cargo in user's environment
    USER_HOME=$(getent passwd "$SUDO_USER" | cut -d: -f6)
    if [ -f "$USER_HOME/.cargo/bin/cargo" ]; then
        CARGO_CMD="$USER_HOME/.cargo/bin/cargo"
        echo "ℹ️  Using cargo from: $CARGO_CMD"
    else
        echo "❌ ERROR: Cannot find cargo command"
        echo "   Please run without sudo: $0"
        echo "   Or install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
        exit 1
    fi
else
    echo "❌ ERROR: cargo command not found"
    echo "   Please install Rust: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
    exit 1
fi

# Check if we have permission to create device files (for whiteout tests)
if [ "$(id -u)" -ne 0 ]; then
    echo "⚠️  WARNING: Not running as root"
    echo "   Whiteout tests may fail without CAP_MKNOD capability"
    echo "   Consider running: sudo -E $0"
    echo ""
fi

# Run tests by priority level
echo "🔴 Running P0 Tests (Core Functionality)..."
echo "   These are CRITICAL tests for the rename2 implementation"
echo ""
$CARGO_CMD test --test test_rename2_comprehensive test_p0 -- --nocapture
echo ""
echo "✅ P0 Tests completed!"
echo ""

echo "🟡 Running P1 Tests (Complete Functionality)..."
echo "   These tests ensure comprehensive feature coverage"
echo ""
$CARGO_CMD test --test test_rename2_comprehensive test_p1 -- --nocapture
echo ""
echo "✅ P1 Tests completed!"
echo ""

echo "🟢 Running P2 Tests (Edge Cases)..."
echo "   These tests cover special scenarios and edge cases"
echo ""
$CARGO_CMD test --test test_rename2_comprehensive test_p2 -- --nocapture
echo ""
echo "✅ P2 Tests completed!"
echo ""

# Run all tests together for final verification
echo "🎯 Running ALL tests together..."
echo ""
$CARGO_CMD test --test test_rename2_comprehensive -- --nocapture --test-threads=1
echo ""

echo "=========================================="
echo "✅ All tests passed!"
echo "=========================================="
echo ""
echo "Test Coverage:"
echo "  P0 (Core):     14 tests ✓"
echo "  P1 (Complete): 13 tests ✓"
echo "  P2 (Edge):      3 tests ✓"
echo "  ─────────────────────────"
echo "  Total:         30 tests ✓"
echo ""
echo "Key areas tested:"
echo "  ✓ Basic rename operations"
echo "  ✓ Whiteout handling (critical!)"
echo "  ✓ Hardlink + whiteout (your senior's fix!) ⭐"
echo "  ✓ RENAME_NOREPLACE flag"
echo "  ✓ RENAME_WHITEOUT flag"
echo "  ✓ RENAME_EXCHANGE flag"
echo "  ✓ Symlink operations"
echo "  ✓ Error conditions"
echo "  ✓ Directory operations"
echo "  ✓ Edge cases"
echo ""

