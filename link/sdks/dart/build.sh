#!/bin/bash
set -euo pipefail

echo "🔨 Building KalamDB Dart SDK..."

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
cd "$SCRIPT_DIR"

echo "📦 Fetching Dart/Flutter dependencies..."
flutter pub get

# Generate flutter_rust_bridge bindings when the bridge config exists.
BRIDGE_DIR="$SCRIPT_DIR/../../kalam-link-dart"
if [ -f "$BRIDGE_DIR/flutter_rust_bridge.yaml" ]; then
  if command -v flutter_rust_bridge_codegen >/dev/null 2>&1; then
    GENERATED_DIR="$SCRIPT_DIR/lib/src/generated"
    STAMP_FILE="$GENERATED_DIR/.frb_codegen.stamp"
    GENERATE_MODE="${FRB_GENERATE:-never}"

    should_generate=false
    if [ "$GENERATE_MODE" = "always" ]; then
      should_generate=true
    elif [ "$GENERATE_MODE" = "never" ]; then
      should_generate=false
    else
      if [ ! -f "$STAMP_FILE" ]; then
        should_generate=true
      elif [ "$BRIDGE_DIR/flutter_rust_bridge.yaml" -nt "$STAMP_FILE" ]; then
        should_generate=true
      elif find "$BRIDGE_DIR/src" -type f -newer "$STAMP_FILE" | grep -q .; then
        should_generate=true
      fi
    fi

    if [ "$should_generate" = true ]; then
      echo "🧬 Generating flutter_rust_bridge bindings..."
      (
        cd "$BRIDGE_DIR"
        flutter_rust_bridge_codegen generate
      )
      touch "$STAMP_FILE"
    else
      echo "🧬 Skipping flutter_rust_bridge generation (disabled or up-to-date)."
      echo "   Override with FRB_GENERATE=always to force regeneration."
    fi
  else
    echo "⚠️ flutter_rust_bridge_codegen not found; skipping binding generation."
    echo "   Install with: dart pub global activate flutter_rust_bridge"
  fi
fi

echo "🔍 Running static analysis..."
flutter analyze

echo "✅ Dart SDK build complete"
