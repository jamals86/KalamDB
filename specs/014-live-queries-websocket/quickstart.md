# Quickstart: Live Queries with Kalam-link

1. Initialize the Kalam-link client with your endpoint and credentials.
2. Call the SDK's `init`/`auth` methods to establish a logical live session.
3. Use `subscribe` to create one or more live query subscriptions; the SDK opens a WebSocket on the first subscription and reuses it.
4. Use `unsubscribe` to cancel individual subscriptions without closing the connection.
5. Optionally call `listSubscriptions` to inspect current active subscriptions.
6. On transient network issues, the SDK automatically reconnects, re-subscribes, and resumes from the last delivered SeqId per subscription.
7. Administrators can inspect active subscriptions via `SELECT * FROM system.live_queries`.
8. Administrators can terminate a connection via `KILL CONNECTION '<connection_id>'`.
