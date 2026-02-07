#!/bin/bash
# Quick test of AI reply functionality

cd /Users/jamal/git/KalamDB/examples/chat-with-ai

# Login
TOKEN=$(curl -s -X POST http://localhost:8080/v1/api/auth/login \
  -H "Content-Type: application/json" \
  -d '{"username": "demo-user", "password": "demo123"}' | jq -r '.access_token')

# Get conversation
CONV_ID=$(curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d '{"sql": "SELECT id FROM chat.conversations LIMIT 1"}' | jq -r '.results[0].rows[0][0]')

echo "📝 Conversation ID: $CONV_ID"
echo ""

# Send message
echo "💬 Sending message..."
curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{\"sql\": \"INSERT INTO chat.messages (conversation_id, sender, role, content) VALUES ($CONV_ID, 'demo-user', 'user', 'Testing AI replies after fix')\"}" \
  | jq -r '.status'

echo ""
echo "⏳ Waiting for topic..."
sleep 2

# Process messages
echo "🤖 Processing messages..."
curl -s -X POST http://localhost:3001/api/process-messages | jq .

echo ""
echo "⏳ Waiting for insert..."
sleep 2

# Check messages
echo ""
echo "📬 Latest messages:"
curl -s -X POST http://localhost:8080/v1/api/sql \
  -H "Content-Type: application/json" \
  -H "Authorization: Bearer $TOKEN" \
  -d "{\"sql\": \"SELECT sender, role, content FROM chat.messages WHERE conversation_id = $CONV_ID ORDER BY created_at DESC LIMIT 3\"}" \
  | jq -r '.results[0].rows[] | "  [\(.[1])] \(.[0]): \(.[2] | tostring | .[0:80])"'

echo ""
