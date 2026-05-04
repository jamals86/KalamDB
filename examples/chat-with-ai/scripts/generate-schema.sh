#!/bin/bash
# Regenerates the typed Drizzle/Kalam schema for the chat_demo namespace.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
PROJECT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
ORM_CLI="$PROJECT_DIR/node_modules/@kalamdb/orm/dist/cli.js"
NODE_FLAGS=(--preserve-symlinks --preserve-symlinks-main)

if [ -f "$PROJECT_DIR/.env.local" ]; then
    set -a
    # shellcheck source=/dev/null
    source "$PROJECT_DIR/.env.local"
    set +a
fi

node "${NODE_FLAGS[@]}" "$ORM_CLI" \
    --url "${KALAMDB_URL:-http://127.0.0.1:8080}" \
    --user "${KALAMDB_USER:-admin}" \
    --password "${KALAMDB_PASSWORD:-kalamdb123}" \
    --namespace chat_demo \
    --include-system-columns \
    --out "$PROJECT_DIR/src/schema.generated.ts"
