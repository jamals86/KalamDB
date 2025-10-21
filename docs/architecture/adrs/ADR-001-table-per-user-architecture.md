# ADR-001: Table-Per-User Architecture

**Date**: 2025-10-20  
**Status**: Accepted  
**Context**: Phase 1-17 (002-simple-kalamdb)

## Context

KalamDB needs to support **millions of concurrent users** with **real-time subscriptions** to their own data changes. Traditional multi-tenant database architectures use shared tables with user_id columns, which creates significant scalability challenges:

1. **Subscription Complexity**: Monitoring a shared table with billions of rows requires complex database triggers or change data capture (CDC) mechanisms that filter by user_id on every write
2. **Query Performance**: Every query must include `WHERE user_id = X` filtering, scanning large indexes
3. **Locking Contention**: Writes to shared tables create lock contention affecting all users
4. **Resource Isolation**: One user's large query or write burst can impact all other users

Example of traditional architecture bottleneck:
```sql
-- Traditional: 1 million users, 1 billion rows in shared table
CREATE TRIGGER notify_user_123 ON messages
FOR EACH INSERT WHERE user_id = 123
  -- Must filter 1 billion rows on every insert
  -- Performance degrades linearly with total user count
```

## Decision

We will use a **table-per-user architecture** where:

1. **User Tables**: Each user gets their own isolated table instance
   - Storage: `RocksDB column family: user_table:{table_name}` (key prefix: `{user_id}:{row_id}`)
   - Parquet files: `{storage_path}/user/{user_id}/{table_name}/batch-*.parquet`

2. **Physical Isolation**: User data is physically separated at the storage layer
   - No shared indexes or table locks
   - Independent flush policies per user
   - Separate Parquet file sets

3. **Subscription Simplicity**: Live queries monitor individual user partitions
   - O(1) complexity per subscription (no shared table filtering)
   - File-level change notifications per user
   - Scales to millions of concurrent subscriptions

## Consequences

### Positive

1. **Massive Scalability**: O(1) subscription complexity per user
   - 1 user with 1,000 rows: Fast
   - 1 million users with 1,000 rows each: Still fast (isolated partitions)
   - Each subscription only monitors its own partition

2. **Simplified Queries**: No user_id filtering required
   ```sql
   -- KalamDB: Query user's own table (no filtering needed)
   SELECT * FROM app.messages ORDER BY timestamp DESC LIMIT 10;
   ```

3. **Resource Isolation**: User operations don't impact each other
   - One user's large write doesn't lock other users' data
   - Flush operations are per-user (no global table flush)

4. **Easy Data Export**: User's complete data in single directory
   ```
   user/{user_id}/messages/*.parquet  # All user's data in one place
   ```

5. **GDPR Compliance**: Simple data deletion (drop user's partition)

6. **Horizontal Scalability**: Add more users without degrading existing user performance

### Negative

1. **More RocksDB Column Families**: Each user table becomes a CF entry
   - Mitigation: RocksDB handles millions of keys efficiently
   - CF operations use key prefixes (no actual per-user CFs)

2. **Metadata Overhead**: Table metadata stored per table type (not per user instance)
   - User tables: Single metadata entry for all users
   - Actual cost: Minimal (metadata is < 1KB)

3. **Cannot Query Across Users**: No global queries on user tables
   - By design: User isolation enforced at storage layer
   - Workaround: Use shared tables for global data

4. **Initial Complexity**: Requires custom DataFusion TableProvider
   - One-time cost: Implemented in Phase 9-10
   - Benefit: Standard SQL interface for applications

### Trade-offs

| Aspect | Traditional Shared Table | KalamDB Per-User Table |
|--------|--------------------------|------------------------|
| **Real-time Subscriptions** | Complex CDC with user filtering | Simple partition monitoring |
| **Concurrent Users** | Degrades with scale | O(1) per user |
| **Query Performance** | Must filter user_id | Direct partition access |
| **Resource Isolation** | Shared locks | Independent operations |
| **Data Export** | Complex user extraction | Simple file copy |
| **Cross-user Queries** | Simple (shared table) | Not possible (by design) |

## Alternatives Considered

### 1. Traditional Shared Tables with CDC

**Pros**:
- Simple schema (single table)
- Easy cross-user queries

**Cons**:
- Does not scale to millions of concurrent subscriptions
- Complex filtering logic for live queries
- Lock contention at scale

**Rejected**: Cannot achieve target scalability (millions of concurrent users).

### 2. Sharding by user_id

**Pros**:
- Some isolation benefits
- Cross-shard queries possible (with distributed query engine)

**Cons**:
- Still requires user_id filtering within shards
- Shard rebalancing complexity
- Distributed query overhead

**Rejected**: Does not eliminate filtering overhead, adds distributed system complexity.

### 3. Hybrid: Shared + Per-User

**Pros**:
- Flexibility for different use cases

**Cons**:
- Dual query patterns (confusing API)
- Still requires filtering on shared tables

**Adopted**: We support this via **three table types**:
- **User Tables**: Per-user isolation
- **Shared Tables**: Global data (for configuration, analytics)
- **Stream Tables**: Ephemeral events (global)

## Implementation Notes

1. **RocksDB Key Structure**:
   ```
   user_table:{table_name}:{user_id}:{row_id} → row_data
   ```
   - Key prefix filtering provides O(1) user isolation
   - No actual per-user column families needed

2. **DataFusion Integration**:
   - `UserTableProvider` implements `TableProvider` trait
   - Automatically filters by current user_id (from session context)
   - Reads from user's RocksDB keys + user's Parquet files

3. **Live Query Architecture**:
   ```rust
   // Monitor user's partition only (not entire table)
   live_query_manager.subscribe(
     user_id: "alice",
     table_name: "messages",
     filter: "timestamp > NOW() - INTERVAL '1 hour'"
   );
   // Only receives notifications for alice's data
   ```

## Related ADRs

- ADR-003: Soft Deletes (user isolation extends to deleted rows)
- ADR-004: RocksDB Column Families (implementation mechanism)
- ADR-009: Three-Layer Architecture (enforces isolation boundaries)

## References

- [Specification](../../specs/002-simple-kalamdb/spec.md) - User Story 2: User Table Creation
- [Tasks](../../specs/002-simple-kalamdb/tasks.md) - Phase 9: User Table CRUD
- [README.md](../../README.md) - Architecture Overview

## Revision History

- 2025-10-20: Initial version (accepted)
