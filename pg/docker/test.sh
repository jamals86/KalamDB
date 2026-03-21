#!/usr/bin/env bash
# ==========================================================================
# KalamDB PostgreSQL Extension — Docker Test Runner
# ==========================================================================
#
# End-to-end test for the pg_kalam FDW extension running in docker-compose.
# Both KalamDB and PostgreSQL run as compose services.
#
# Prerequisites:
#   1. Build the extension image:
#      ./pg/docker/build.sh
#   2. Start compose services:
#      cd pg/docker && docker compose up -d
#
# Environment variables:
#   PGHOST           — PostgreSQL host     (default: localhost)
#   PGPORT           — PostgreSQL port     (default: 5433)
#   PGUSER           — PostgreSQL user     (default: kalamdb)
#   PGPASSWORD       — PostgreSQL password (default: kalamdb123)
#   PGDATABASE       — PostgreSQL database (default: kalamdb)
#   KALAMDB_API_URL  — KalamDB HTTP API    (default: http://localhost:8088)
#   KALAMDB_PASSWORD — Admin password      (default: kalamdb123)
# ==========================================================================
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

export PGHOST="${PGHOST:-localhost}"
export PGPORT="${PGPORT:-5433}"
export PGUSER="${PGUSER:-kalamdb}"
export PGPASSWORD="${PGPASSWORD:-kalamdb123}"
export PGDATABASE="${PGDATABASE:-kalamdb}"

KALAMDB_API_URL="${KALAMDB_API_URL:-http://localhost:8088}"
KALAMDB_PASSWORD="${KALAMDB_PASSWORD:-kalamdb123}"

if [ -x "$HOME/.pgrx/16.13/pgrx-install/bin/pg_isready" ]; then
    PG_ISREADY_BIN="$HOME/.pgrx/16.13/pgrx-install/bin/pg_isready"
else
    PG_ISREADY_BIN="$(command -v pg_isready)"
fi

if [ -x "$HOME/.pgrx/16.13/pgrx-install/bin/psql" ]; then
    PSQL_BIN="$HOME/.pgrx/16.13/pgrx-install/bin/psql"
else
    PSQL_BIN="$(command -v psql)"
fi

echo "========================================"
echo " KalamDB PG Extension — Docker Test"
echo "========================================"
echo " PG Host:     $PGHOST:$PGPORT"
echo " PG Database: $PGDATABASE"
echo " PG User:     $PGUSER"
echo " KalamDB API: $KALAMDB_API_URL"
echo "========================================"
echo ""

# Step 1: Check KalamDB server is reachable
echo "Checking KalamDB server at $KALAMDB_API_URL ..."
for i in $(seq 1 15); do
    if curl -sf "$KALAMDB_API_URL/health" > /dev/null 2>&1; then
        echo "KalamDB server is reachable."
        break
    fi
    if [ "$i" -eq 15 ]; then
        echo "ERROR: KalamDB server at $KALAMDB_API_URL is not reachable."
        echo "Start it with: cd backend && cargo run"
        exit 1
    fi
    sleep 2
done

echo ""
echo "Bootstrapping KalamDB namespace 'rmtest' and table 'profiles' ..."

LOGIN_RESP=$(curl -sf "$KALAMDB_API_URL/v1/api/auth/login" \
    -H "Content-Type: application/json" \
    -d "{\"username\":\"admin\",\"password\":\"$KALAMDB_PASSWORD\"}" \
    2>/dev/null || true)

BEARER_TOKEN=$(echo "$LOGIN_RESP" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
if [ -z "$BEARER_TOKEN" ]; then
    echo "WARNING: Could not login to KalamDB. Trying setup first..."
    curl -sf "$KALAMDB_API_URL/v1/api/auth/setup" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"admin\",\"password\":\"$KALAMDB_PASSWORD\",\"root_password\":\"$KALAMDB_PASSWORD\"}" \
        > /dev/null 2>&1 || true
    LOGIN_RESP=$(curl -sf "$KALAMDB_API_URL/v1/api/auth/login" \
        -H "Content-Type: application/json" \
        -d "{\"username\":\"admin\",\"password\":\"$KALAMDB_PASSWORD\"}" \
        2>/dev/null || true)
    BEARER_TOKEN=$(echo "$LOGIN_RESP" | grep -o '"access_token":"[^"]*"' | cut -d'"' -f4)
fi

if [ -z "$BEARER_TOKEN" ]; then
    echo "ERROR: Could not authenticate with KalamDB server."
    exit 1
fi

curl -s "$KALAMDB_API_URL/v1/api/sql" \
    -H "Authorization: Bearer $BEARER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"sql": "CREATE NAMESPACE IF NOT EXISTS rmtest"}' > /dev/null

curl -s "$KALAMDB_API_URL/v1/api/sql" \
    -H "Authorization: Bearer $BEARER_TOKEN" \
    -H "Content-Type: application/json" \
    -d '{"sql": "CREATE USER TABLE IF NOT EXISTS rmtest.profiles (id TEXT PRIMARY KEY, name TEXT, age INTEGER)"}' > /dev/null

echo "Waiting for PostgreSQL to be ready..."
for i in $(seq 1 30); do
    if "$PG_ISREADY_BIN" -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" > /dev/null 2>&1; then
        echo "PostgreSQL is ready."
        break
    fi
    if [ "$i" -eq 30 ]; then
        echo "ERROR: PostgreSQL did not become ready within 30 seconds."
        echo "Is the container running?  cd pg/docker && docker compose up -d"
        exit 1
    fi
    sleep 1
done

echo ""
echo "Running test.sql ..."
echo ""

PAGER=cat "$PSQL_BIN" -h "$PGHOST" -p "$PGPORT" -U "$PGUSER" -d "$PGDATABASE" \
    -v ON_ERROR_STOP=1 \
    -P pager=off \
    -f "$SCRIPT_DIR/test.sql"

echo ""
echo "========================================"
echo " All tests passed!"
echo "========================================"
