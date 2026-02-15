#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
CRATE_DIR="$(cd "${SCRIPT_DIR}/.." && pwd)"
SCHEMA_DIR="${CRATE_DIR}/src/serialization/schema"
OUT_DIR="${CRATE_DIR}/src/serialization/generated"

if ! command -v flatc >/dev/null 2>&1; then
  echo "flatc is required. Install with: brew install flatbuffers" >&2
  exit 1
fi

mkdir -p "${OUT_DIR}"

flatc --rust -o "${OUT_DIR}" \
  "${SCHEMA_DIR}/entity_envelope.fbs" \
  "${SCHEMA_DIR}/row_models.fbs"

echo "Generated FlatBuffer bindings in ${OUT_DIR}"
