# Data Model: Live Queries over WebSocket Connections

## Entities

### WebSocket Connection
- **Description**: Represents a long-lived connection between a client and the KalamDB backend.
- **Key Fields**:
  - `connection_id` (string/UUID): Stable identifier for the connection.
  - `user_id` / `tenant_id`: Link to authenticated principal.
  - `created_at`, `last_activity_at`: Timestamps for lifecycle management.
  - `state`: {`open`, `closing`, `closed`, `killed`}. 

### Live Query Subscription
- **Description**: A single live query bound to a specific WebSocket connection.
- **Key Fields**:
  - `connection_id`: Foreign key to WebSocket Connection.
  - `subscription_id`: Identifier unique within the connection.
  - `query`: Stored query text or reference.
  - `status`: {`active`, `unsubscribed`, `terminated`}.
  - `created_at`: Creation time.
  - `last_seq_id`: Last sequence identifier delivered to the client (used on resume).

### System Live Queries View (`system.live_queries`)
- **Description**: System table/view exposing active live query subscriptions.
- **Key Fields**:
  - `(connection_id, subscription_id)`: Composite identifier for a subscription.
  - `user_id` / `tenant_id`: Owning principal.
  - `query`: Text or identifier.
  - `status`: Active/terminated/unsubscribed indicator.
  - `created_at`, `last_seq_id`.

## Relationships

- One **WebSocket Connection** to many **Live Query Subscriptions** (1:N).
- Each **Live Query Subscription** appears as a row in `system.live_queries` while active.

## Validation & Constraints

- `(connection_id, subscription_id)` MUST be unique across active subscriptions.
- Subscriptions MUST belong to an authenticated user/tenant via their connection.
- When a connection is killed or auth expires, all its subscriptions MUST transition to a non-active status and be removed or marked inactive in `system.live_queries`.
- `last_seq_id` MUST be monotonic per subscription when updates are delivered.
