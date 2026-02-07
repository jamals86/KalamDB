#!/bin/bash
# Recreate tables as SHARED instead of USER

cd /Users/jamal/git/KalamDB/examples/chat-with-ai

echo "🔄 Recreating tables as SHARED..."

TOKEN=$(curl -s -X POST http://localhost:8080/v1/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "root", "password": "kalamdb123"}' | jq -r '.access_token')

# Drop old USER tables
echo "Dropping old tables..."
curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
 -d '{"sql": "DROP TABLE IF EXISTS chat.conversations"}' | jq -r '.status'

curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "DROP TABLE IF EXISTS chat.messages"}' | jq -r '.status'

curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "DROP TABLE IF EXISTS chat.typing_indicators"}' | jq -r '.status'

echo ""
echo "Creating SHARED tables..."

# Create SHARED tables
curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "CREATE SHARED TABLE chat.conversations (id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID() PRIMARY KEY, title TEXT NOT NULL, created_by TEXT NOT NULL, created_at TIMESTAMP NOT NULL DEFAULT NOW(), updated_at TIMESTAMP NOT NULL DEFAULT NOW())"}' | jq -r '.status'

curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "CREATE SHARED TABLE chat.messages (id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID() PRIMARY KEY, conversation_id BIGINT NOT NULL, sender TEXT NOT NULL, role TEXT NOT NULL DEFAULT '\''user'\'', content TEXT NOT NULL, status TEXT NOT NULL DEFAULT '\''sent'\'', created_at TIMESTAMP NOT NULL DEFAULT NOW())"}' | jq -r '.status'

curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "CREATE SHARED TABLE chat.typing_indicators (id BIGINT NOT NULL DEFAULT SNOWFLAKE_ID() PRIMARY KEY, conversation_id BIGINT NOT NULL, user_name TEXT NOT NULL, is_typing BOOLEAN NOT NULL DEFAULT TRUE, updated_at TIMESTAMP NOT NULL DEFAULT NOW())"}' | jq -r '.status'

# Recreate topic source
echo ""
echo "Recreating topic source..."
curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "ALTER TOPIC chat.new_messages DROP SOURCE chat.messages"}' > /dev/null 2>&1

curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "ALTER TOPIC chat.new_messages ADD SOURCE chat.messages ON INSERT WITH (payload = '\''full'\'')"}' | jq -r '.status'

# Add sample conversations
echo ""
echo "Adding sample data..."
curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "INSERT INTO chat.conversations (title, created_by) VALUES ('\''Welcome to KalamDB Chat'\'', '\''demo-user'\''), ('\''Getting Started with AI'\'', '\''demo-user'\'')"}' | jq -r '.status'

echo ""
echo "✅ Done! Tables recreated as SHARED."
echo "   All users can now see all messages including AI replies."
