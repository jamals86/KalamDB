#!/bin/bash
set -euo pipefail

echo "🧪 Testing KalamDB Dart SDK..."

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "📦 Ensuring dependencies are installed..."
flutter pub get

echo "🧭 Running analyzer checks..."
flutter analyze

echo "🧪 Running test suite..."
flutter test

echo "✅ All Dart SDK tests passed"
