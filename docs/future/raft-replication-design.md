## Canvas: KalamDB Cluster Replication Spec (Phase 1 + Phase 2)

**Status:** Proposed (to replace / supersede “Recommended Approach” section)
**Created:** Jan 2026
**Scope:** Replication + cluster semantics + SQL/CLI behavior (SQL-first)

KalamDB is a SQL-first database with a hot tier (RocksDB) and a cold tier (Parquet + manifest) and a core orchestration layer that talks to storage only through `kalamdb-store` and `kalamdb-filestore`. 

---

# 1. Decision Summary

We will implement **a single Raft group for the control plane (system catalog)** and run **a single-primary data plane** in Phase 1.

* **Control Plane (strongly consistent, HA):** replicated via Raft

  * namespaces, tables, schema versions, storages, users/roles, and other low-churn catalog metadata
* **Data Plane (Phase 1):** single-primary (leader-only) for all DML + subscriptions

  * leader executes DML and serves subscriptions
  * followers forward/redirect to the leader
  * cold tier must be shared across nodes for meaningful failover

This gives KalamDB an immediate “real cluster” operational story **without putting hot writes behind quorum latency**.

---

# 2. Goals and Non‑Goals

## Goals (Phase 1)

1. **Strong consistency for catalog changes** (DDL, user/storage config) using Raft. 
2. **Automatic catch-up** of offline nodes using Raft log + snapshots.
3. **Single-primary semantics for DML** so KalamDB behaves like a single logical database (no silent data divergence).
4. **SQL-first cluster introspection** via system tables (and CLI helpers), consistent with existing `system.*` patterns like `system.tables`.  
5. **Keep architecture simple** and consistent with the existing crate boundary rule: `kalamdb-core` orchestrates; storage access is via `kalamdb-store` and `kalamdb-filestore` (no direct RocksDB usage from `core`). 

## Non‑Goals (Phase 1)

* Multi-Raft (per-table/per-range Raft groups)
* Cross-node transactions / distributed SQL
* Multi-writer active/active hot tier writes
* Strong consistency of **hot** DML across node failures (this is Phase 2)

---

# 3. Cluster Modes

KalamDB runs in exactly one of these modes:

## 3.1 Standalone Mode

* No Raft
* Local system catalog + local data
* Existing behavior continues unchanged

## 3.2 Cluster Mode (Phase 1)

* Raft enabled for the system catalog
* One node is the **leader** (Raft leader)
* **All SQL statements that can observe or change hot data** are routed to leader

---

# 4. Phase 1 Architecture

## 4.1 Control Plane: “Metadata Raft Group”

**Single Raft group** replicates a deterministic state machine that writes to the **system catalog**.

### Replicated Entities (control plane only)

Replicate **low-churn metadata**:

* Namespaces (`CREATE/DROP NAMESPACE`) 
* Tables (definitions, schema versions, options like FLUSH_POLICY, TTL, etc.) 
* Storages (filesystem / s3 configuration) 
* Users, roles, auth metadata 
* (future) Views, RBAC policies, reserved-name registry

### Not replicated via Raft in Phase 1

* **High-churn data-plane artifacts**:

  * per-user / per-table manifest content (batch lists)
  * job progress, live query rows, etc.

Reason: manifests can update frequently (flush-heavy workloads) and would bloat the Raft log.

## 4.2 Data Plane: Single‑Primary

In cluster mode Phase 1:

* **Leader executes all DML** (`INSERT/UPDATE/DELETE`) and produces live query events.
* **Followers are read-only nodes** and **must forward** or **reject with a leader hint**.

This keeps behavior consistent with KalamDB’s SQL-first surface and avoids the “each node has different data” trap.

## 4.3 Cold Storage Requirement (Cluster Mode)

To make leader failover meaningful, **cold storage must be shared** (S3/object store or shared filesystem path) so any leader can read flushed Parquet + manifests. KalamDB already models cold storage via storage backends (filesystem/S3) and a folder layout with `manifest.json` next to batches.  

---

# 5. Durability and Consistency Guarantees

## 5.1 Catalog (DDL / control plane)

**Guaranteed:**

* Linearizable catalog changes (as seen by nodes that have applied the committed log).
* After a successful DDL response, any new leader elected from the quorum will contain that catalog change.

## 5.2 Data (DML / hot tier)

**Phase 1 guarantee:**

* DML is **linearizable only through the leader** (single-primary).
* DML is **durable on the leader’s local disk** (RocksDB), but **not replicated** to followers.

**Important limitation (must be explicit):**

* If the leader is permanently lost, **unflushed hot data may be lost**.
* Flushed cold data (Parquet + manifest) remains safe if stored on shared object storage.

This matches KalamDB’s two-tier design (hot buffer + cold Parquet) and current operational model where cold tier is the durable long-term store. 

---

# 6. Catalog Revision Model (Required for correctness + caching)

Every committed Raft entry increments a **monotonic catalog revision**.

* `catalog_revision = last_applied_log_index` (or equivalent)
* Persisted and exposed for clients/CLI for cache invalidation

This helps:

* schema cache refresh
* plan cache invalidation
* UI “what changed?” / debugging

---

# 7. SQL Semantics in Cluster Mode (Phase 1)

KalamDB is SQL-first and already documents DDL/DML, SUBSCRIBE, FLUSH, and catalog browsing patterns. 

## 7.1 Statement routing rules

| Statement Class          | Examples                                                                                         | Allowed on Follower? | Behavior on Follower                               |
| ------------------------ | ------------------------------------------------------------------------------------------------ | -------------------: | -------------------------------------------------- |
| **DDL (catalog writes)** | `CREATE NAMESPACE`, `CREATE TABLE`, `ALTER TABLE`, `DROP TABLE`, `CREATE STORAGE`, `CREATE USER` |      ✅ (via routing) | Forward to leader (preferred) or error w/ hint     |
| **DML (hot writes)**     | `INSERT`, `UPDATE`, `DELETE`, `FLUSH TABLE`                                                      |      ✅ (via routing) | Forward to leader or error w/ hint                 |
| **Subscriptions**        | `SUBSCRIBE TO ...`                                                                               |      ✅ (via routing) | Return response with leader `ws_url` (no WS proxy) |
| **Reads**                | `SELECT ...`                                                                                     |      ✅ (via routing) | Forward to leader (Phase 1 default)                |

**Phase 1 default:** forward *everything* to leader. This keeps semantics identical to single-node behavior and avoids “stale reads” surprises.

> Later (Phase 2 or Phase 1.5), we can add an opt-in for stale follower reads (cold-only) if needed.

## 7.2 “Not leader” error shape

When forwarding is disabled or fails, return a Postgres/MySQL-like error message:

* CLI:

  * `ERROR: node is not leader`
  * `HINT: connect to leader at http://<leader_host>:<port>`
* SQL API JSON:

  * `code: "NOT_LEADER"`
  * `message: "node is not leader"`
  * `details: "leader_url=http://..."`

Keep it simple and consistent.

## 7.3 SUBSCRIBE behavior in cluster mode

KalamDB’s SQL reference returns a `ws_url` when a subscription is required. 
In cluster mode:

* If request hits leader: return leader `ws_url`
* If request hits follower: return leader `ws_url` (and optionally a hint that node is follower)

This avoids building a WebSocket proxy in Phase 1 and keeps the SDK behavior clean. 

---

# 8. Cluster Introspection via System Tables (SQL‑First)

KalamDB already exposes catalog browsing via `system.tables` (unified TableDefinition model). 
We add **two** new system tables:

## 8.1 `system.cluster_members`

Columns (suggested):

* `node_id` TEXT PK
* `advertise_addr` TEXT
* `raft_addr` TEXT
* `role` TEXT (`leader` | `follower` | `learner`)
* `term` BIGINT
* `last_heartbeat_at` TIMESTAMP
* `last_applied` BIGINT
* `catalog_revision` BIGINT
* `healthy` BOOLEAN

## 8.2 `system.raft_status`

Single-row table (suggested):

* `cluster_id` TEXT
* `leader_id` TEXT
* `term` BIGINT
* `commit_index` BIGINT
* `last_applied` BIGINT
* `snapshot_index` BIGINT
* `log_size_bytes` BIGINT

### Example queries

```sql
SELECT * FROM system.cluster_members ORDER BY node_id;
SELECT * FROM system.raft_status;
```

---

# 9. CLI Behavior (Phase 1)

The `kalam` CLI already supports meta commands (`\dt`, `\d`, `\health`, etc.). 
Add three meta-commands that simply run SQL:

* `\cluster` → `SELECT * FROM system.cluster_members ORDER BY node_id;`
* `\leader` → `SELECT leader_id, term, catalog_revision FROM system.raft_status;`
* `\raft` → `SELECT * FROM system.raft_status;`

**Important:** These are *thin* wrappers—no special client logic required.

---

# 10. Bootstrap and Membership

## 10.1 Phase 1 Membership Model: Static

Phase 1 uses **static membership** from config:

* simple
* predictable
* fewer correctness pitfalls

## 10.2 Required node identity

Each node has:

* `node_id` (stable across restart, unique in cluster)
* `cluster_id` (UUID, shared by cluster)

This also aligns with KalamDB’s existing need for stable node IDs (e.g., Snowflake IDs depend on a node identifier). 

## 10.3 Example config (sketch)

```toml
[cluster]
mode = "cluster"
cluster_id = "c0c91a1c-...."
node_id = "node-1"
advertise_addr = "http://10.0.0.11:8080"

[cluster.raft]
bind_addr = "10.0.0.11:9090"
members = [
  { node_id="node-1", raft_addr="10.0.0.11:9090", advertise_addr="http://10.0.0.11:8080" },
  { node_id="node-2", raft_addr="10.0.0.12:9090", advertise_addr="http://10.0.0.12:8080" },
  { node_id="node-3", raft_addr="10.0.0.13:9090", advertise_addr="http://10.0.0.13:8080" },
]
```

---

# 11. Manifest + Flush Rules (Cluster Mode)

KalamDB’s cold storage layout places `manifest.json` next to Parquet batches. 

## Phase 1 rule

* **Manifests remain data-plane artifacts** written by the flush job into cold storage.
* We do **not** replicate manifest contents via Raft.
* For discovery, other nodes read manifests directly from shared storage.

If we later need a “fast pointer,” replicate only:

* `{table_id, latest_manifest_generation}`
  not the full list of batches.

---

# 12. Implementation Boundaries (Align with KalamDB crates)

KalamDB’s crate graph and principle is clear:
`kalamdb-core` orchestrates; storage is accessed via `kalamdb-store` (RocksDB) and `kalamdb-filestore` (Parquet + manifests). 

## 12.1 Crates/modules (recommended)

* `backend/crates/kalamdb-raft/` (new)

  * openraft integration
  * Raft network + Raft log storage traits
  * state machine applies catalog commands via **store abstractions**
* `backend/crates/kalamdb-core/`

  * `cluster/` module:

    * leader routing (forward-or-hint)
    * cluster status queries → exposed through system tables
  * SchemaRegistry / StorageRegistry / Auth repo route catalog mutations through Raft

## 12.2 Hard rule

**No RocksDB direct usage outside `kalamdb-store`.**
Raft log storage must be implemented using a storage abstraction provided by `kalamdb-store`. 

---

# 13. Phase 2: Data Replication Options (Future)

Phase 2 is about making DML survive leader loss (replicating hot tier).

We support **one** of these, chosen by simplicity first:

## Option A (recommended): WAL shipping / async replication

* Leader writes to RocksDB + emits an ordered WAL stream
* Followers ingest WAL and apply to their local hot tier
* Can be:

  * async (fast, slight lag)
  * semi-sync (quorum-ish durability without full Raft)

**Pros:** simpler than multi-raft, preserves hot tier perf
**Cons:** more bespoke correctness work

## Option B: “Critical tables use Raft” (selective multi-raft)

* Keep metadata raft group
* Add optional per-table raft group for “must-not-lose” datasets

**Pros:** strong guarantees where needed
**Cons:** complexity (many raft groups), scheduling

## Option C: Shared hot storage backend

* Replace/augment hot tier to write into shared storage (or a replicated cache)
* Leverage object_store abstraction work (already in roadmap) 

**Pros:** operational simplicity
**Cons:** changes hot-path performance characteristics

---

# 14. What this gives KalamDB immediately

With Phase 1 implemented, KalamDB gains:

* Real **clustered catalog**: DDL and user/storage changes are consistent and HA.
* Clear operational semantics: **one leader** executes DML and subscriptions.
* Minimal performance impact on hot writes (no quorum latency).
* Simple introspection via SQL and the existing CLI command style.  

And most importantly, it stays aligned with KalamDB’s core principle: **simple, inspectable architecture with a SQL-first interface.**  
