# Feature Specification: Live Queries over WebSocket Connections

**Feature Branch**: `014-live-queries-websocket`  
**Created**: November 24, 2025  
**Status**: Draft  
**Input**: User description: "Each user can open multiple websocket connections, each websocket connection can have multiple subscription queries in it, each query will have a row in system.live_queries which has a conneciton_id and subscription_id which is unique and the query i think we already have this

the user can open websocket and subscribe to query then subscribe to different query which are all in the same websocket connection at once a websocket connection can have multiple subscription queries in it

the admin can kill a connection which will kill all its subscription inside

teh user can be able to unsubscribe from one query without closing the connection or affecting the other subscription queries it has


Another change for the Kalam-link which should also gives ability to:
1) init a connection
2) auth for that connection
3) subscribe for a query
4) unsubscribe for a query
5) view all subscriptions at once
These will give us ability for the sdk to subscribe to a live query
The web socket will be opened on first subscription only
Second subscription will use the same opened socket"

## User Scenarios & Testing *(mandatory)*

### User Story 1 - Manage multiple subscriptions on a single WebSocket (Priority: P1)

An authenticated application user opens a live connection to KalamDB, subscribes to one query, and then adds additional live query subscriptions on the same WebSocket connection. They can independently unsubscribe from any one subscription without closing the connection or impacting the other active subscriptions.

**Why this priority**: This is the core value of live queries for applications, enabling efficient, multiplexed real-time data updates without the overhead of many separate connections.

**Independent Test**: Start a client session, establish a single WebSocket connection, create two distinct live query subscriptions, and verify that unsubscribing from one leaves the other subscription active and still receiving updates.

**Acceptance Scenarios**:

1. **Given** an authenticated user with an open WebSocket connection and one active subscription, **When** they subscribe to a second query on the same connection, **Then** both subscriptions are active and receive updates independently.
2. **Given** an authenticated user with two active subscriptions on the same connection, **When** they issue an unsubscribe request for one subscription, **Then** that subscription stops receiving updates while the other continues uninterrupted.

---

### User Story 2 - Track live queries in system.live_queries (Priority: P2)

An administrator or observability tool can inspect system metadata to see all active live query subscriptions, including which WebSocket connection and subscription identifier each belongs to, allowing them to reason about current live load and manage problematic sessions.

**Why this priority**: System-level visibility into active live queries is required for debugging, monitoring, and administration, and underpins kill-connection behavior.

**Independent Test**: With at least one active WebSocket connection and multiple subscriptions, query the `system.live_queries` table and verify that each subscription is represented with the correct connection identifier, unique subscription identifier, and associated query details.

**Acceptance Scenarios**:

1. **Given** an active WebSocket connection with multiple live query subscriptions, **When** `system.live_queries` is queried, **Then** there is one row per active subscription with a unique `(connection_id, subscription_id)` combination and accurate query metadata.
2. **Given** an existing live query subscription row in `system.live_queries`, **When** the client unsubscribes from that subscription, **Then** the corresponding row is removed or marked inactive within a bounded delay.

---

### User Story 3 - Admin terminates a live connection (Priority: P3)

An administrator identifies a problematic or abusive WebSocket connection and terminates that connection, causing all associated live query subscriptions on that connection to be stopped and reflected as no longer active in system metadata.

**Why this priority**: Administrative control over live connections is important for stability, cost control, and security in multi-tenant environments.

**Independent Test**: Create one user with a WebSocket connection and multiple subscriptions, then use an administrative capability to kill that connection and verify that all subscriptions stop receiving updates and that `system.live_queries` reflects the termination.

**Acceptance Scenarios**:

1. **Given** a WebSocket connection with multiple active subscriptions, **When** an admin issues a kill-connection operation for that connection, **Then** the WebSocket is closed and all subscriptions on it are terminated.
2. **Given** an admin-killed connection, **When** observing `system.live_queries` after a short delay, **Then** no active rows remain for that connection.

---

### User Story 4 - SDK client manages connection lifecycle (Priority: P4)

An SDK consumer (e.g., Kalam-link TypeScript client) uses a high-level API to initialize a logical connection, authenticate once, and then create, list, and cancel live query subscriptions without manually managing raw WebSocket details. The SDK opens the WebSocket on the first subscription and reuses it for subsequent subscriptions.

**Why this priority**: This provides a clean, ergonomic integration path for client applications, hiding protocol details and enabling reuse of a single connection for multiple subscriptions.

**Independent Test**: From an SDK-based client, call `init` and `auth` (or equivalent), create two live query subscriptions, list current subscriptions, then unsubscribe from one and verify that the underlying WebSocket remains open and the remaining subscription still functions.

**Acceptance Scenarios**:

1. **Given** an application using the SDK with no active subscriptions, **When** it calls the first `subscribe` operation, **Then** the SDK opens a WebSocket connection, authenticates it, and establishes the subscription.
2. **Given** an application using the SDK with one active subscription, **When** it calls `subscribe` again for a different query, **Then** the SDK reuses the same WebSocket connection and both subscriptions become active.
3. **Given** an application with multiple active subscriptions created via the SDK, **When** it calls `unsubscribe` for one subscription, **Then** only that subscription is removed while the others and the WebSocket connection remain active.
4. **Given** an application with multiple active subscriptions created via the SDK, **When** it calls a `listSubscriptions`/"view all subscriptions" style method, **Then** it receives a list of currently active subscriptions including their identifiers and associated queries.
5. **Given** an application using the TypeScript SDK, **When** it invokes the Kalam-link shared library helpers for connect/disconnect/subscribe/unsubscribe/resume, **Then** the shared library performs the lifecycle logic and the SDK simply forwards inputs/outputs without duplicating that logic.

### Edge Cases

- What happens when a client attempts to subscribe to a new query after the server has closed the WebSocket due to inactivity or error? The SDK should transparently reopen and re-authenticate the connection before creating new subscriptions.
- How does the system handle a client unsubscribing from a subscription that is already inactive or unknown (e.g., double-unsubscribe or stale identifier)? The operation should be idempotent and return a clear, non-fatal result.
- What happens to active subscriptions when the underlying auth for a connection expires or is revoked? Active subscriptions on that connection SHOULD be terminated promptly so that no further updates are delivered on an unauthorized connection, and the same teardown/cleanup SLA as admin kill (≤5 seconds) MUST apply.

## Requirements *(mandatory)*

### Functional Requirements

- **FR-001**: The system MUST allow a single authenticated user to maintain multiple concurrent WebSocket connections, each of which can be independently opened and closed.
- **FR-002**: The system MUST allow each WebSocket connection to host multiple independent live query subscriptions at the same time.
- **FR-003**: The system MUST ensure that each live query subscription has a unique subscription identifier within the scope of its connection and that the pair `(connection_id, subscription_id)` is unique across all active rows in `system.live_queries`.
- **FR-004**: The system MUST persist each live query subscription row on creation, update it with required metadata (e.g., SeqId) during the subscription lifecycle, and immediately remove that row whenever the subscription unsubscribes, the WebSocket closes, an admin kill occurs, or authentication expires—ensuring `system.live_queries` is accurate within 5 seconds of any event.
- **FR-005**: The system MUST provide an operation that allows an administrator to terminate a specific WebSocket connection, which MUST close the connection and stop all associated subscriptions.
- **FR-006**: The system MUST ensure that terminating a connection via admin action results in all associated `system.live_queries` entries being removed or marked inactive within 5 seconds under normal operating conditions.
- **FR-007**: When authentication for a connection expires or is revoked, the system MUST close the WebSocket and cascade the same cleanup steps (including `system.live_queries` removal) within 5 seconds.
- **FR-008**: The client-facing API MUST support unsubscribing from a single live query subscription without closing the underlying WebSocket connection or affecting other active subscriptions on that connection.
- **FR-009**: The SDK for Kalam-link MUST provide capabilities to (1) initialize a logical live connection, (2) authenticate that connection, (3) subscribe to a live query, (4) unsubscribe from a live query, and (5) retrieve a list of all current subscriptions associated with that client session.
- **FR-010**: The SDK MUST open the underlying WebSocket connection lazily on the first subscription request and reuse the same open connection for subsequent subscriptions from the same logical client context.
- **FR-011**: Connection lifecycle management MUST live inside the Kalam-link shared library so that every SDK (including TypeScript) consumes the same tooling for connect, disconnect, subscribe, unsubscribe, and resume flows.
- **FR-012**: The SDK MUST handle reconnection behavior when the WebSocket is unexpectedly closed, including re-establishing authentication and automatically re-subscribing all previously active subscriptions after a successful reconnect.
- **FR-013**: The SDK MUST remember, per subscription, the last received sequence identifier (SeqId) from the server and, on reconnect, pass this identifier so that the server can resume the stream from that point in time and avoid duplicate or missing updates.
- **FR-014**: The system MUST maintain O(1) lookup structures (e.g., hash maps keyed by `(user_id, connection_id)`) so that locating the WebSocket handler for any user table change never requires scanning unrelated users or connections.
- **FR-015**: The system MUST expose enough metadata (e.g., connection identifiers and associated user/tenant information) to allow administrators to identify and manage individual connections for monitoring and kill-connection operations.

## Clarifications

### Session 2025-11-24

- Q: How should the SDK handle live query subscriptions after an unexpected WebSocket disconnect and reconnect? → A: Automatically re-subscribe all previously active subscriptions and have the client pass the last received SeqId per subscription so the server can resume from that point.

### Key Entities *(include if feature involves data)*

- **WebSocket Connection**: Represents a live, long-lived connection between a client and the server. Key attributes include connection identifier, associated user/tenant, authentication status, creation time, last activity time, and current state (open, closing, closed, killed by admin).
- **Live Query Subscription**: Represents a single live query bound to a WebSocket connection. Key attributes include subscription identifier (scoped to a connection), connection identifier, query text or reference, creation time, status (active, unsubscribed, terminated), and optional metadata for resubscription behavior.
- **System Live Queries View (`system.live_queries`)**: A system-level representation of active live query subscriptions. Key attributes include `(connection_id, subscription_id)` pair, user/tenant identifiers, query details, and subscription status, enabling observability and administration.

## Success Criteria *(mandatory)*

### Measurable Outcomes

- **SC-001**: An application user can maintain at least two simultaneous live query subscriptions on a single WebSocket connection and independently unsubscribe one without affecting the other in 100% of tested scenarios.
- **SC-002**: Administrative tools can reliably list and terminate specific WebSocket connections such that all associated live query subscriptions stop receiving updates and disappear from `system.live_queries` within 5 seconds in normal operating conditions (same target applies to auth expiry cleanup).
- **SC-003**: SDK consumers can initialize, authenticate, subscribe, unsubscribe, and list subscriptions using high-level APIs such that at least 90% of new integrators can complete a basic live query integration (one connection, two subscriptions, one unsubscribe) within one hour following documentation.
- **SC-004**: Under typical load test conditions, reuse of a single WebSocket connection for multiple subscriptions reduces the number of concurrent connections per client by at least 50% compared to a one-connection-per-subscription baseline, without degrading the correctness of delivered updates.
- **SC-005**: Automated integration tests cover the full lifecycle (multi-subscribe, unsubscribe, resume with SeqId, view subscriptions, kill subscription) on a single WebSocket connection and run green in CI.
