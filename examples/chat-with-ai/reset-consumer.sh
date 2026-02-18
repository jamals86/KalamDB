#!/usr/bin/env bash
# Reset consumer group offset to reprocess all messages

SERVER_URL="${KALAMDB_URL:-http://localhost:8080}"

echo "Resetting consumer group 'ai-processor-service' for topic 'chat.ai-processing'..."

curl -s -u root:kalamdb123 "$SERVER_URL/v1/api/query" \
  -H "Content-Type: application/json" \
  -d "{\"query\": \"DELETE FROM system.topic_offsets WHERE topic_id = 'chat.ai-processing' AND group_id = 'ai-processor-service'\"}" | jq '.results[0].row_count // .error'

echo ""
echo "Consumer offset reset. The consumer will now start from the beginning on next run."
