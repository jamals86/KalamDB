#!/usr/bin/env bash
# publish.sh — Publish kalam_link to pub.dev
# Usage: ./publish.sh [--dry-run]
set -euo pipefail

DRY_RUN=false
SKIP_NATIVE_CHECK=false
for arg in "$@"; do
  case "$arg" in
    --dry-run) DRY_RUN=true ;;
    --skip-native-check) SKIP_NATIVE_CHECK=true ;;
  esac
done

cd "$(dirname "$0")"

# ── Validate that native artefacts exist for every declared platform ──────
if ! $SKIP_NATIVE_CHECK; then
  echo "==> Checking native library artefacts..."
  MISSING=()
  # Android (at least arm64-v8a + x86_64)
  [[ -f android/src/main/jniLibs/arm64-v8a/libkalam_link_dart.so ]]  || MISSING+=("android/arm64-v8a")
  [[ -f android/src/main/jniLibs/x86_64/libkalam_link_dart.so ]]     || MISSING+=("android/x86_64")
  # iOS
  [[ -f ios/Frameworks/libkalam_link_dart.a ]]                        || MISSING+=("ios")
  # Web WASM
  [[ -f web/pkg/kalam_link_dart_bg.wasm ]]                            || MISSING+=("web/wasm")

  if [[ ${#MISSING[@]} -gt 0 ]]; then
    echo ""
    echo "ERROR: Missing pre-built native libraries for: ${MISSING[*]}"
    echo "Run './build_native_libs.sh all' to compile them, then commit."
    echo "Or pass --skip-native-check to bypass this validation."
    exit 1
  fi
  echo "    All native artefacts present."
fi

echo "==> Checking git working tree..."
if [[ -n "$(git status --porcelain -- .)" ]]; then
  echo "ERROR: Uncommitted changes detected in this package directory."
  echo "Commit or stash them before publishing."
  git status --short -- .
  exit 1
fi

echo "==> Running flutter pub get..."
flutter pub get

echo "==> Running dart analyze..."
dart analyze lib

echo "==> Running tests..."
flutter test || true   # warn but don't block (live-server tests may require a running server)

if $DRY_RUN; then
  echo "==> Dry run — validating package (no upload)..."
  flutter pub publish --dry-run
  echo ""
  echo "Dry run complete. Run './publish.sh' (without --dry-run) to publish."
else
  echo "==> Publishing kalam_link to pub.dev..."
  echo "y" | flutter pub publish
fi
