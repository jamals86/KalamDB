#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SDK_DIR="$(cd "$PROJECT_DIR/../../link/sdks/typescript" && pwd)"
WASM_FILE="$SDK_DIR/dist/wasm/kalam_link_bg.wasm"
SDK_ENTRY="$SDK_DIR/dist/src/index.js"

needs_build=0
if [[ ! -f "$WASM_FILE" || ! -f "$SDK_ENTRY" ]]; then
  needs_build=1
fi

if [[ "$needs_build" -eq 0 ]]; then
  if find "$SDK_DIR/src" "$SDK_DIR/../../src" -type f -newer "$SDK_ENTRY" | head -n 1 | grep -q .; then
    needs_build=1
  fi
fi

if [[ "$needs_build" -eq 1 ]]; then
  echo "[ensure-sdk] Building local kalam-link SDK..."
  (
    cd "$SDK_DIR"
    bash ./build.sh
  )
  echo "[ensure-sdk] SDK build complete"
else
  echo "[ensure-sdk] Local kalam-link SDK is up to date"
fi
