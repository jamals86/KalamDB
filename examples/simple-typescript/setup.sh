#!/bin/bash
# Setup script for KalamDB TODO example
# This script validates KalamDB accessibility and creates the todos table

set -e  # Exit on error

# Color output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

echo "🚀 Setting up KalamDB TODO example..."
echo

# Check if .env file exists
if [ ! -f .env ]; then
    echo -e "${YELLOW}⚠️  No .env file found${NC}"
    echo "   Creating .env from .env.example..."
    cp .env.example .env
    echo -e "${YELLOW}   Please edit .env and add your API key before continuing${NC}"
    echo
    exit 1
fi

# Load environment variables
source .env

# Validate required environment variables
if [ -z "$VITE_KALAMDB_URL" ]; then
    echo -e "${RED}❌ VITE_KALAMDB_URL is not set in .env${NC}"
    exit 1
fi

if [ -z "$VITE_KALAMDB_API_KEY" ] || [ "$VITE_KALAMDB_API_KEY" = "your-api-key-here" ]; then
    echo -e "${RED}❌ VITE_KALAMDB_API_KEY is not set or using placeholder value${NC}"
    echo "   Run the following to create a user and get an API key:"
    echo "   cargo run --bin kalamdb-server -- create-user --username todo-app --email todo@example.com --role user"
    exit 1
fi

echo "📡 Checking KalamDB server accessibility..."

# Check if server is running (try health endpoint)
HTTP_CODE=$(curl -s -o /dev/null -w "%{http_code}" "${VITE_KALAMDB_URL}/health" || echo "000")

if [ "$HTTP_CODE" != "200" ]; then
    echo -e "${RED}❌ Cannot reach KalamDB server at ${VITE_KALAMDB_URL}${NC}"
    echo "   HTTP status: $HTTP_CODE"
    echo "   Make sure the server is running with: cargo run --bin kalamdb-server"
    exit 1
fi

echo -e "${GREEN}✅ KalamDB server is accessible${NC}"
echo

# Find kalam-cli binary
KALAM_CLI=""
if [ -f "../../target/release/kalam-cli" ]; then
    KALAM_CLI="../../target/release/kalam-cli"
elif [ -f "../../target/debug/kalam-cli" ]; then
    KALAM_CLI="../../target/debug/kalam-cli"
elif command -v kalam-cli &> /dev/null; then
    KALAM_CLI="kalam-cli"
else
    echo -e "${RED}❌ kalam-cli binary not found${NC}"
    echo "   Please build it first with: cd ../../cli && cargo build --release"
    exit 1
fi

echo "📋 Creating database schema..."

# Execute SQL file using kalam-cli
export KALAMDB_URL="$VITE_KALAMDB_URL"
export KALAMDB_API_KEY="$VITE_KALAMDB_API_KEY"

if $KALAM_CLI execute-file todo-app.sql; then
    echo -e "${GREEN}✅ Database schema created successfully${NC}"
else
    echo -e "${RED}❌ Failed to create database schema${NC}"
    exit 1
fi

echo
echo -e "${GREEN}🎉 Setup complete!${NC}"
echo
echo "Next steps:"
echo "  1. Install dependencies: npm install"
echo "  2. Start the dev server: npm run dev"
echo "  3. Open http://localhost:3000 in your browser"
echo
