#!/bin/bash
# Verify chrono version to prevent regression of the Arrow 52.2.0 conflict
# Run this script in CI or before commits to ensure chrono stays pinned to 0.4.39

set -e

echo "🔍 Checking chrono version..."

# Get chrono version from dependency tree
chrono_version=$(cargo tree -i chrono | grep "^chrono v" | head -n 1)

if echo "$chrono_version" | grep -q "chrono v0\.4\.39"; then
    echo "✅ SUCCESS: chrono is correctly pinned to 0.4.39"
    
    # Check for duplicates
    if cargo tree -i chrono -d 2>&1 | grep -q "warning: nothing to print"; then
        echo "✅ SUCCESS: No duplicate chrono versions found"
        echo ""
        echo "All checks passed! Arrow 52.2.0 conflict is resolved."
        exit 0
    else
        echo "⚠️  WARNING: Multiple chrono versions detected!"
        cargo tree -i chrono -d
        exit 1
    fi
elif echo "$chrono_version" | grep -qE "chrono v0\.4\.([4-9][0-9]|40)"; then
    echo "❌ FAILED: chrono is at version 0.4.40+ which conflicts with arrow-arith 52.2.0"
    echo "   Current version: $chrono_version"
    echo ""
    echo "To fix, run:"
    echo "  cargo update -p chrono --precise 0.4.39"
    echo ""
    echo "See backend/KNOWN_ISSUES.md for details."
    exit 1
else
    echo "⚠️  WARNING: Unexpected chrono version detected"
    echo "   Current version: $chrono_version"
    echo "   Expected: chrono v0.4.39"
    exit 1
fi
