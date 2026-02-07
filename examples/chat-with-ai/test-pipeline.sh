#!/bin/bash
# Test the complete chat pipeline

echo "🧪 Testing KalamDB Chat Pipeline"
echo "================================="
echo ""

# Test 1: Check Next.js is running
echo "1️⃣  Testing Next.js API..."
response=$(curl -s http://localhost:3001/api/conversations)
if echo "$response" | jq . > /dev/null 2>&1; then
    echo "   ✅ Next.js API is working"
    count=$(echo "$response" | jq 'length')
    echo "   📊 Found $count conversations"
else
    echo "   ❌ Next.js API failed"
    exit 1
fi

echo ""

# Test 2: Check message processing endpoint
echo "2️⃣  Testing message processor API..."
result=$(curl -s -X POST http://localhost:3001/api/process-messages)
if echo "$result" | jq -e '.success' > /dev/null 2>&1; then
    echo "   ✅ Process messages API is working"
    processed=$(echo "$result" | jq -r '.processed')
    available=$(echo "$result" | jq -r '.available')
    echo "   📊 Processed: $processed, Available: $available"
else
    echo "   ❌ Process messages API failed"
    exit 1
fi

echo ""

# Test 3: Send a test message
echo "3️⃣  Testing full message flow..."
TOKEN=$(curl -s -X POST http://localhost:8080/v1/api/auth/login \
    -H "Content-Type: application/json" \
    -d '{"username": "demo-user", "password": "demo123"}' | jq -r '.access_token')

if [ -z "$TOKEN" ] || [ "$TOKEN" = "null" ]; then
    echo "   ❌ Failed to get auth token"
    exit 1
fi

# Get first conversation
CONV_ID=$(curl -s -X POST http://localhost:8080/v1/api/sql \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d '{"sql": "SELECT id FROM chat.conversations ORDER BY created_at DESC LIMIT 1"}' \
    | jq -r '.results[0].rows[0][0]')

echo "   📝 Using conversation: $CONV_ID"

# Send message
MSG_RESULT=$(curl -s -X POST http://localhost:8080/v1/api/sql \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "{\"sql\": \"INSERT INTO chat.messages (conversation_id, sender, role, content) VALUES ($CONV_ID, 'demo-user', 'user', 'Test message from script')\"}" \
    | jq -r '.status')

if [ "$MSG_RESULT" = "success" ]; then
    echo "   ✅ Message inserted successfully"
else
    echo "   ❌ Failed to insert message"
    exit 1
fi

# Process messages
sleep 2
PROCESS_RESULT=$(curl -s -X POST http://localhost:3001/api/process-messages | jq -r '.processed')
echo "   🤖 AI processed $PROCESS_RESULT messages"

# Check for reply
sleep 2
MESSAGES=$(curl -s -X POST http://localhost:8080/v1/api/sql \
    -H "Content-Type: application/json" \
    -H "Authorization: Bearer $TOKEN" \
    -d "{\"sql\": \"SELECT sender, role, LEFT(content, 50) FROM chat.messages WHERE conversation_id = $CONV_ID ORDER BY created_at DESC LIMIT 3\"}" \
    | jq -r '.results[0].rows[] | "[\(.[1])] \(.[0]): \(.[2])"')

echo "   📬 Recent messages:"
echo "$MESSAGES" | while read -r line; do
    echo "      $line"
done

echo ""
echo "================================="
echo "✅ All tests passed!"
echo ""
echo "🌐 Open in browser:"
echo "   Design 1: http://localhost:3001/design1"
echo "   Design 2: http://localhost:3001/design2"
echo "   Design 3: http://localhost:3001/design3"
