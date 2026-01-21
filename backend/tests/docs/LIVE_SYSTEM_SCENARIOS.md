# Live Queries & System Observability Test Scenarios [CATEGORY:LIVE_SYSTEM]

This document covers real-time subscriptions, TTL eviction, and system metadata.

## 1. WebSocket Live Subscriptions
**Goal**: Real-time push of data changes to clients.
- **Scenario A**: Filtered Subscriptions.
  1. Subscribe to `SELECT * FROM chat.messages WHERE conversation_id = 123`.
  2. Insert messages for `123` and `456`.
  3. Verify ONLY messages for `123` are pushed.
- **Scenario B**: Initial Snapshot + Live.
  1. Table has 100 existing rows.
  2. Client subscribes with `initial_data: true`.
  3. Verify client receives `initial_data_batch` messages followed by live events.
- **Agent Friendly**: Check `backend/tests/testserver/subscription/test_live_query_inserts.rs`.

## 2. Stream TTL Eviction
**Goal**: Automatic cleanup of ephemeral data.
- **Steps**:
  1. Create a STREAM table with `TTL_SECONDS = 5`.
  2. Insert rows.
  3. Wait 6 seconds.
  4. Verify rows are no longer returned in queries and have been physically removed (or marked for eviction).
- **Agent Friendly**: Check `backend/tests/testserver/subscription/test_stream_ttl_eviction_sql.rs`.

## 3. System Tables & Audit Logs
**Goal**: Observability into the database state.
- **Steps**:
  1. Query `system.tables`, `system.namespaces`, `system.jobs`.
  2. Perform a sensitive operation (e.g., DROP TABLE).
  3. Verify entry in `system.audit_logs`.
- **Agent Friendly**: Check `backend/tests/testserver/system/test_system_tables_http.rs`.

## 4. Offline Sync (Batched Snapshot)
**Goal**: Handle large initial data transfers efficiently.
- **Steps**:
  1. Preload 5,000 rows.
  2. Subscribe with `batch_size: 1000`.
  3. Verify receipt of exactly 5 batches of `initial_data`.
  4. Verify `has_more` flag transitions correctly.
- **Agent Friendly**: Check `backend/tests/scenarios/scenario_02_offline_sync.rs`.

## Coverage Analysis
- **Duplicates**: Subscription tests for Insert/Update/Delete are split into different files but share 80% setup code.
- **Gaps**: Backpressure handling for fast-writing clients in WebSocket streams is not explicitly stressed in a "slow consumer" scenario.
