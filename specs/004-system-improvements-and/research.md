# Research: System Improvements and Performance Optimization

**Feature Branch**: `004-system-improvements-and`  
**Created**: October 21, 2025  
**Phase**: Phase 0 - Research and Requirements Clarification

## Purpose

This document resolves all "NEEDS CLARIFICATION" items identified in the Technical Context of plan.md. Each research item provides:
- **Decision**: What was chosen
- **Rationale**: Why it was chosen
- **Alternatives Considered**: What else was evaluated

---

## Research Item 1: kalam-link API Design for Subscription Events

**Context**: kalam-link must provide a way for kalam-cli and other consumers to receive WebSocket subscription events (INSERT, UPDATE, DELETE notifications). The interface must work in both native Rust and WebAssembly contexts.

**Decision**: Use async Stream trait (`impl Stream<Item = Result<ChangeEvent>>`) as the primary interface with builder pattern for subscription configuration.

```rust
// kalam-link API
pub struct KalamLinkClient { /* ... */ }

impl KalamLinkClient {
    pub async fn subscribe(&self, query: &str) -> Result<Subscription> {
        // Returns Subscription handle
    }
}

pub struct Subscription {
    events: Pin<Box<dyn Stream<Item = Result<ChangeEvent>> + Send>>,
    // ...
}

impl Subscription {
    pub fn events(&mut self) -> impl Stream<Item = Result<ChangeEvent>> + '_ {
        &mut self.events
    }
    
    pub async fn close(&mut self) -> Result<()> {
        // Graceful WebSocket close
    }
}

pub enum ChangeEvent {
    Insert { data: RecordBatch },
    Update { old: RecordBatch, new: RecordBatch },
    Delete { data: RecordBatch },
}
```

**Rationale**:
1. **Stream trait is standard**: Rust async ecosystem (tokio, futures) uses Stream as the idiomatic pattern for async iteration
2. **Composability**: Consumers can use standard stream combinators (filter, map, take, etc.)
3. **Backpressure**: Stream trait naturally handles backpressure (slow consumer pauses WebSocket reads)
4. **WebAssembly compatible**: Stream trait works in wasm with wasm-compatible runtimes (tokio + wasm feature)
5. **Cancellation**: Dropping the Subscription handle automatically closes WebSocket (RAII pattern)

**Alternatives Considered**:
- **Callback with channel**: `subscribe(query, callback: impl Fn(ChangeEvent))` - Rejected because callbacks don't compose well, no backpressure control, harder to test
- **Polling API**: `subscription.next_event().await` - Rejected because it's lower-level and requires manual loop management in every consumer
- **Observable pattern**: RxRust-style observables - Rejected because it adds a large dependency and Stream trait provides equivalent functionality

**Example Usage**:
```rust
// In kalam-cli
let mut subscription = client.subscribe("SELECT * FROM messages WHERE user_id = 'jamal'").await?;
let mut events = subscription.events();

while let Some(event) = events.next().await {
    match event? {
        ChangeEvent::Insert { data } => println!("New message: {:?}", data),
        ChangeEvent::Update { old, new } => println!("Updated: {:?} -> {:?}", old, new),
        ChangeEvent::Delete { data } => println!("Deleted: {:?}", data),
    }
}
```

---

## Research Item 2: Session Cache Eviction Policy

**Context**: Session-level table registration cache must evict unused registrations to prevent unbounded memory growth. Policy must balance hit rate (performance) with memory usage.

**Decision**: **Hybrid LRU + TTL policy** with configurable parameters:
- LRU eviction when cache reaches max size (default: 100 registrations per session)
- TTL-based eviction for idle registrations (default: 30 minutes)
- Immediate eviction on schema version mismatch

```rust
pub struct SessionCache {
    registrations: LruCache<TableKey, CachedRegistration>,
    ttl: Duration,
    max_size: usize,
}

struct CachedRegistration {
    table: Arc<dyn TableProvider>,
    schema_version: u64,
    last_accessed: Instant,
}

impl SessionCache {
    pub fn get(&mut self, key: &TableKey) -> Option<Arc<dyn TableProvider>> {
        // Check TTL
        if let Some(reg) = self.registrations.get(key) {
            if reg.last_accessed.elapsed() > self.ttl {
                self.registrations.pop(key);
                return None;
            }
            // Validate schema version (consult metadata store)
            if !self.validate_schema_version(key, reg.schema_version) {
                self.registrations.pop(key);
                return None;
            }
            Some(reg.table.clone())
        } else {
            None
        }
    }
}
```

**Rationale**:
1. **LRU handles size bounds**: Prevents memory exhaustion with hard limit on cache entries
2. **TTL handles idle sessions**: Evicts stale registrations even if cache isn't full (handles long-lived sessions with changing table access patterns)
3. **Schema validation**: Immediate eviction on schema changes ensures correctness over performance
4. **Configurable**: Different workloads can tune max_size and ttl (e.g., OLTP: small cache with short TTL; OLAP: large cache with long TTL)

**Alternatives Considered**:
- **Pure LRU**: No TTL - Rejected because long-lived sessions could hold stale registrations indefinitely if tables are accessed infrequently
- **Pure TTL**: No size limit - Rejected because high table churn could cause unbounded cache growth within TTL window
- **FIFO**: First-in-first-out - Rejected because it doesn't prioritize frequently-used tables (poor hit rate)
- **Random eviction**: Rejected because unpredictable performance (hot tables could be evicted)

**Configuration** (config.toml):
```toml
[session_cache]
max_registrations_per_session = 100
ttl_seconds = 1800  # 30 minutes
```

---

## Research Item 3: Storage Trait Method Signatures

**Context**: Storage abstraction trait must support both RocksDB (column families) and simple key-value stores (Redis, Sled) without exposing backend-specific details. Trait must cover all current RocksDB operations.

**Decision**: Define trait with explicit column family parameter (abstracted as `Partition`) for backends that support it, with default implementation for simple KV stores.

```rust
pub trait StorageBackend: Send + Sync {
    /// Get a value by key from a specific partition (column family in RocksDB)
    async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>>;
    
    /// Put a key-value pair in a specific partition
    async fn put(&self, partition: &Partition, key: &[u8], value: &[u8]) -> Result<()>;
    
    /// Delete a key from a specific partition
    async fn delete(&self, partition: &Partition, key: &[u8]) -> Result<()>;
    
    /// Scan a range of keys in a partition
    async fn scan(
        &self,
        partition: &Partition,
        start_key: Option<&[u8]>,
        end_key: Option<&[u8]>,
    ) -> Result<Box<dyn Iterator<Item = Result<(Vec<u8>, Vec<u8>)>> + Send>>;
    
    /// Atomic batch operations
    async fn batch(&self, operations: Vec<Operation>) -> Result<()>;
    
    /// Create a partition (column family in RocksDB, key prefix in others)
    async fn create_partition(&self, partition: &Partition) -> Result<()>;
    
    /// Check if partition exists
    async fn partition_exists(&self, partition: &Partition) -> Result<bool>;
}

pub struct Partition {
    name: String,
}

pub enum Operation {
    Put { partition: Partition, key: Vec<u8>, value: Vec<u8> },
    Delete { partition: Partition, key: Vec<u8> },
}
```

**Rationale**:
1. **Partition abstraction**: Column families (RocksDB) map directly to Partition; simple KV stores implement Partition as key prefix (e.g., "partition:key")
2. **Async trait**: Enables async storage backends (network-based stores like Redis)
3. **Iterator for scan**: Avoids loading entire scan results into memory (streaming)
4. **Batch operations**: Enables atomic multi-key updates (critical for consistency)

**Backend Implementations**:

**RocksDB**:
```rust
pub struct RocksDbBackend {
    db: Arc<rocksdb::DB>,
}

impl StorageBackend for RocksDbBackend {
    async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
        let cf = self.db.cf_handle(&partition.name).ok_or(...)?;
        Ok(self.db.get_cf(cf, key)?)
    }
    // ... other methods use cf_handle
}
```

**Sled** (simple KV store):
```rust
pub struct SledBackend {
    db: sled::Db,
}

impl StorageBackend for SledBackend {
    async fn get(&self, partition: &Partition, key: &[u8]) -> Result<Option<Vec<u8>>> {
        // Sled doesn't have column families - use key prefix
        let prefixed_key = format!("{}:{}", partition.name, hex::encode(key));
        Ok(self.db.get(prefixed_key.as_bytes())?.map(|v| v.to_vec()))
    }
    // ... other methods prepend partition name to key
}
```

**Alternatives Considered**:
- **No partition parameter**: All keys managed manually with prefixes - Rejected because it moves partitioning logic into every call site (error-prone, violates DRY)
- **Separate traits for partitioned vs non-partitioned**: `PartitionedStorage` and `SimpleStorage` - Rejected because it complicates generic code (need separate code paths for each trait)
- **Partition as Option<&str>**: Optional partition parameter - Rejected because KalamDB always uses partitioned data (no reason to support partition-less operations)

---

## Research Item 4: Docker Base Image Selection

**Context**: Docker image must balance size, security, and compatibility. Alpine is small but uses musl libc (compatibility issues). Distroless is secure but harder to debug. Debian/Ubuntu are large but highly compatible.

**Decision**: **Multi-stage build with Debian bookworm-slim for builder, distroless/cc for runtime**.

```dockerfile
# Stage 1: Builder (full toolchain)
FROM rust:1.75-bookworm AS builder
WORKDIR /app
COPY . .
RUN cargo build --release --bin kalamdb-server

# Stage 2: Runtime (minimal distroless)
FROM gcr.io/distroless/cc-debian12
COPY --from=builder /app/target/release/kalamdb-server /usr/local/bin/
COPY --from=builder /app/config.example.toml /etc/kalamdb/config.toml
ENTRYPOINT ["/usr/local/bin/kalamdb-server"]
CMD ["--config", "/etc/kalamdb/config.toml"]
```

**Rationale**:
1. **Debian builder**: Full compatibility with RocksDB, Parquet, and other native dependencies (no musl libc issues)
2. **Distroless runtime**: Minimal attack surface (no shell, no package manager), small size (~20MB base + binary)
3. **Multi-stage**: Builder artifacts (toolchain, build cache) not included in final image
4. **gcr.io/distroless/cc**: Includes libc and OpenSSL (required for RocksDB and JWT libraries)

**Alternatives Considered**:
- **Alpine Linux**: Small size (5MB base) - Rejected due to musl libc compatibility issues with RocksDB (requires custom compilation)
- **Debian/Ubuntu slim**: ~40MB base - Rejected because distroless is more secure (no shell, no unnecessary packages)
- **Scratch** (empty base): Smallest possible - Rejected because KalamDB requires libc and OpenSSL (won't run on scratch)
- **Full Rust image**: ~1GB - Rejected because it includes entire Rust toolchain in runtime image (security risk, large size)

**Expected Image Size**: ~50MB (20MB distroless base + 30MB KalamDB binary after stripping)

**Security Considerations**:
- Distroless images are maintained by Google with CVE scanning
- No shell means no remote code execution via shell injection
- Minimal dependencies reduce attack surface
- Use specific image tags (not `latest`) for reproducible builds

---

## Research Item 5: Sharding Function Plugin Mechanism

**Context**: Flush operations must distribute data across shards. Default is alphabetic (a-z), but custom sharding strategies should be possible for advanced users (e.g., geo-based, hash-based, range-based).

**Decision**: **Trait-based plugin system with registration at server startup**.

```rust
pub trait ShardingStrategy: Send + Sync {
    fn shard_key(&self, key: &str) -> String;
    fn shard_count(&self) -> usize;
}

pub struct AlphabeticSharding {
    shards: Vec<String>,  // ["a", "b", ..., "z"]
}

impl ShardingStrategy for AlphabeticSharding {
    fn shard_key(&self, key: &str) -> String {
        let first_char = key.chars().next().unwrap_or('z').to_ascii_lowercase();
        if first_char.is_ascii_lowercase() {
            first_char.to_string()
        } else {
            "z".to_string()  // Default shard for non-alpha
        }
    }
    
    fn shard_count(&self) -> usize { 26 }
}

pub struct ConsistentHashSharding {
    shard_count: usize,
}

impl ShardingStrategy for ConsistentHashSharding {
    fn shard_key(&self, key: &str) -> String {
        let hash = seahash::hash(key.as_bytes());
        let shard_id = hash % self.shard_count as u64;
        format!("shard-{:04}", shard_id)
    }
    
    fn shard_count(&self) -> usize { self.shard_count }
}

// Registration
pub struct ShardingRegistry {
    strategies: HashMap<String, Arc<dyn ShardingStrategy>>,
}

impl ShardingRegistry {
    pub fn register(&mut self, name: &str, strategy: Arc<dyn ShardingStrategy>) {
        self.strategies.insert(name.to_string(), strategy);
    }
    
    pub fn get(&self, name: &str) -> Option<Arc<dyn ShardingStrategy>> {
        self.strategies.get(name).cloned()
    }
}
```

**Configuration** (config.toml):
```toml
[sharding]
default_strategy = "alphabetic"

# Future: Plugin-loaded strategies
# [[sharding.custom]]
# name = "geo"
# library = "/path/to/libgeo_sharding.so"
```

**Rationale**:
1. **Trait-based**: Standard Rust pattern for plugins (compile-time or dynamic loading)
2. **Send + Sync**: Enables use in concurrent contexts (flush jobs run in parallel)
3. **Stateless**: Shard calculation is pure function of key (no mutable state)
4. **Extensible**: Users can implement trait for custom strategies (compile into server or load as dylib)

**Alternatives Considered**:
- **Function pointer**: `type ShardingFn = fn(&str) -> String` - Rejected because it can't carry state (e.g., shard count configuration)
- **Enum with variants**: `enum Sharding { Alphabetic, Hash(usize), Geo(GeoConfig) }` - Rejected because it requires modifying enum for every new strategy (not extensible)
- **Dynamic library loading**: Load `.so`/`.dll` at runtime - Deferred to future enhancement (complex, requires ABI stability)
- **Embedded scripting**: Lua/Rhai for user-defined sharding - Rejected because it adds large dependency and introduces security risks

**Default Strategies Provided**:
1. **alphabetic**: First letter (a-z), default shard for non-alpha
2. **numeric**: First digit (0-9)
3. **hash**: Consistent hash with configurable shard count

---

## Research Item 6: Query Plan Cache Key Structure

**Context**: Query plan cache must generate stable keys for SQL queries such that structurally identical queries (with different parameter values) map to the same cached plan.

**Decision**: **Use DataFusion's LogicalPlan AST hash after parameter normalization**.

```rust
pub struct QueryPlanCache {
    cache: LruCache<QueryKey, Arc<LogicalPlan>>,
}

#[derive(Hash, Eq, PartialEq)]
struct QueryKey {
    normalized_sql: String,
    schema_hash: u64,  // Table schema versions involved
}

impl QueryPlanCache {
    pub fn get_or_compile(
        &mut self,
        sql: &str,
        ctx: &SessionContext,
    ) -> Result<Arc<LogicalPlan>> {
        // Step 1: Normalize SQL (replace parameter values with placeholders)
        let normalized = self.normalize_sql(sql)?;
        
        // Step 2: Compute schema hash (detect schema changes)
        let schema_hash = self.compute_schema_hash(&normalized, ctx)?;
        
        let key = QueryKey { normalized_sql: normalized, schema_hash };
        
        // Step 3: Check cache
        if let Some(plan) = self.cache.get(&key) {
            return Ok(plan.clone());
        }
        
        // Step 4: Parse and optimize plan
        let plan = ctx.sql(&sql).await?.into_optimized_plan()?;
        let arc_plan = Arc::new(plan);
        self.cache.put(key, arc_plan.clone());
        Ok(arc_plan)
    }
    
    fn normalize_sql(&self, sql: &str) -> Result<String> {
        // Replace parameter values with $1, $2, ...
        // Example: "SELECT * FROM t WHERE id = 123" -> "SELECT * FROM t WHERE id = $1"
        let mut normalized = sql.to_string();
        // Use regex or SQL parser to find literals and replace
        // (Details in implementation - use sqlparser-rs)
        Ok(normalized)
    }
    
    fn compute_schema_hash(&self, sql: &str, ctx: &SessionContext) -> Result<u64> {
        // Parse SQL to extract table names
        let tables = extract_table_names(sql)?;
        // Get schema versions for each table
        let mut hasher = DefaultHasher::new();
        for table in tables {
            let schema_version = ctx.get_table_schema_version(&table)?;
            hasher.write_u64(schema_version);
        }
        Ok(hasher.finish())
    }
}
```

**Rationale**:
1. **SQL normalization**: Structurally identical queries with different literal values map to same key
2. **Schema hash**: Invalidates cache when table schemas change (correctness over performance)
3. **LRU eviction**: Prevents unbounded cache growth (configurable max size)
4. **DataFusion integration**: Leverages DataFusion's built-in plan optimization (no custom optimizer)

**Normalization Strategy**:
- **Parameter placeholders**: Replace literal values with $1, $2, ... (ORDER BY column is NOT normalized - structural difference)
- **Whitespace normalization**: `SELECT  *  FROM t` == `SELECT * FROM t`
- **Case normalization**: `select` == `SELECT` (SQL is case-insensitive)
- **Comment removal**: `/* comment */ SELECT` == `SELECT`

**Alternatives Considered**:
- **String-based cache key**: Hash of original SQL - Rejected because `SELECT * FROM t WHERE id = 123` and `id = 456` would be separate cache entries
- **AST serialization**: Serialize DataFusion AST to JSON and hash - Rejected because AST includes parameter values (doesn't solve the problem)
- **Manual query fingerprinting**: Custom algorithm to compute query "fingerprint" - Rejected because it duplicates DataFusion's SQL parsing logic (error-prone)

**Cache Configuration** (config.toml):
```toml
[query_cache]
max_plans = 1000
eviction_policy = "lru"
```

---

## Research Item 7: Flush Job Actor Communication Protocol

**Context**: Flush jobs run as actors. The scheduler must be able to start, cancel, and query status of flush jobs. Actors must report progress and errors back to the scheduler.

**Decision**: **Message-based protocol using tokio channels with job lifecycle states**.

```rust
pub enum FlushJobMessage {
    Start { table: TableName, namespace: NamespaceId },
    Cancel { job_id: JobId },
    GetStatus { job_id: JobId, respond_to: oneshot::Sender<JobStatus> },
}

pub enum JobStatus {
    Running { progress: f32, records_processed: usize },
    Completed { records_flushed: usize, storage_location: String },
    Failed { error: String },
    Cancelled,
}

pub struct FlushJobActor {
    receiver: mpsc::Receiver<FlushJobMessage>,
    jobs: HashMap<JobId, JoinHandle<Result<()>>>,
}

impl FlushJobActor {
    pub async fn run(&mut self) {
        while let Some(msg) = self.receiver.recv().await {
            match msg {
                FlushJobMessage::Start { table, namespace } => {
                    let job_id = JobId::new();
                    let handle = tokio::spawn(async move {
                        self.execute_flush(table, namespace).await
                    });
                    self.jobs.insert(job_id, handle);
                }
                FlushJobMessage::Cancel { job_id } => {
                    if let Some(handle) = self.jobs.remove(&job_id) {
                        handle.abort();
                    }
                }
                FlushJobMessage::GetStatus { job_id, respond_to } => {
                    let status = self.query_status(job_id);
                    let _ = respond_to.send(status);
                }
            }
        }
    }
}
```

**Scheduler Integration**:
```rust
pub struct FlushScheduler {
    actor_tx: mpsc::Sender<FlushJobMessage>,
}

impl FlushScheduler {
    pub async fn schedule_flush(&self, table: TableName) -> Result<JobId> {
        let job_id = JobId::new();
        self.actor_tx.send(FlushJobMessage::Start {
            table,
            namespace: /* ... */,
        }).await?;
        Ok(job_id)
    }
    
    pub async fn cancel_flush(&self, job_id: JobId) -> Result<()> {
        self.actor_tx.send(FlushJobMessage::Cancel { job_id }).await?;
        Ok(())
    }
    
    pub async fn get_job_status(&self, job_id: JobId) -> Result<JobStatus> {
        let (tx, rx) = oneshot::channel();
        self.actor_tx.send(FlushJobMessage::GetStatus { job_id, respond_to: tx }).await?;
        Ok(rx.await?)
    }
}
```

**Rationale**:
1. **Message passing**: Decouples scheduler from job execution (actor pattern)
2. **Bounded channels**: Prevents unbounded queue growth (backpressure)
3. **oneshot for responses**: Type-safe request-response pattern
4. **Cancellation**: Flush jobs can be aborted (important for long-running flushes during shutdown)
5. **Status tracking**: Scheduler can query job progress for observability (system.jobs table)

**Alternatives Considered**:
- **Shared state with Mutex**: Jobs update shared HashMap<JobId, JobStatus> - Rejected because it requires locking on every status update (contention)
- **Direct function calls**: No actors, synchronous flush - Rejected because it blocks the scheduler thread (can't handle multiple concurrent flushes)
- **Actor framework** (actix, tokio-actor): Use existing actor library - Rejected because it adds unnecessary complexity for simple message passing (tokio channels are sufficient)

**Job Lifecycle**:
1. **Created**: Job ID generated, added to system.jobs table
2. **Running**: Actor spawns async task, updates progress periodically
3. **Completed/Failed/Cancelled**: Final status written to system.jobs, task cleaned up

---

## Research Item 8: DataFusion Expression Serialization for Live Query Filters

**Context**: Live query subscriptions cache DataFusion expressions for filter evaluation. If the server restarts, cached expressions are lost and must be recompiled from the original SQL filter string.

**Decision**: **Store original SQL filter string with each subscription; recompile expressions on server restart**.

```rust
pub struct LiveQuerySubscription {
    live_id: LiveQueryId,
    filter_sql: String,  // Original SQL: "user_id = 'jamal' AND created_at > '2025-01-01'"
    cached_expr: Option<Arc<Expr>>,  // Compiled DataFusion expression (in-memory only)
}

impl LiveQuerySubscription {
    pub fn evaluate_filter(&mut self, record_batch: &RecordBatch) -> Result<RecordBatch> {
        // Lazy compile if not cached
        if self.cached_expr.is_none() {
            let expr = self.compile_filter(&self.filter_sql)?;
            self.cached_expr = Some(Arc::new(expr));
        }
        
        let expr = self.cached_expr.as_ref().unwrap();
        filter_record_batch(record_batch, expr)
    }
    
    fn compile_filter(&self, sql: &str) -> Result<Expr> {
        // Use DataFusion's SQL parser to parse WHERE clause
        let ctx = SessionContext::new();
        let parsed = ctx.parse_sql_expr(sql)?;
        Ok(parsed)
    }
}
```

**Persistence Strategy**:
```rust
// system.live_queries table stores:
// - live_id: UUID
// - filter_sql: TEXT (original SQL WHERE clause)
// - options: JSON (subscription options)
// On server restart:
let subscriptions = load_subscriptions_from_db()?;
for sub in subscriptions {
    // cached_expr starts as None, will be compiled on first use
    reconnect_websocket(sub);
}
```

**Rationale**:
1. **SQL as source of truth**: Original SQL filter is human-readable and stable (expression AST is implementation detail)
2. **Lazy recompilation**: Expressions compiled on first use after restart (amortizes startup cost)
3. **No serialization complexity**: DataFusion Expr doesn't implement Serialize (custom serialization would be fragile across DataFusion version updates)
4. **Crash resilience**: Server restart doesn't lose subscription state (SQL filter is durable)

**Alternatives Considered**:
- **Serialize DataFusion Expr**: Implement custom serialization - Rejected because DataFusion Expr is complex enum with many variants; serialization would break across DataFusion version updates
- **Store compiled bytecode**: Serialize to custom bytecode format - Rejected because it requires maintaining bytecode interpreter (high complexity, low benefit)
- **No persistence**: Lose subscriptions on restart - Rejected because it provides poor user experience (clients must reestablish subscriptions after every restart)
- **Expression caching across restarts**: Serialize to custom cache file - Rejected because it adds complexity without clear benefit (recompilation takes <10ms per expression)

**Performance Impact**:
- **Cold start**: ~10ms per subscription to recompile filter expression
- **After compilation**: <1ms per filter evaluation (same as pre-restart)
- **Typical system**: 1000 active subscriptions = 10 seconds total recompilation time on startup (acceptable)

---

## Summary of Decisions

| Research Item | Decision | Key Rationale |
|---------------|----------|---------------|
| kalam-link subscription API | async Stream trait | Standard Rust async pattern, composable, WebAssembly compatible |
| Session cache eviction | Hybrid LRU + TTL | Balances memory usage (LRU) with staleness prevention (TTL) |
| Storage trait signatures | Partition-based trait with async | Abstracts column families, supports both RocksDB and simple KV stores |
| Docker base image | Multi-stage: Debian builder + distroless runtime | Security (minimal attack surface) + compatibility (glibc) |
| Sharding plugin mechanism | Trait-based registration | Extensible, stateless, supports future dynamic loading |
| Query plan cache key | Normalized SQL + schema hash | Structurally identical queries share plan, schema changes invalidate |
| Flush job actor protocol | tokio channels with message enum | Decoupled, cancellable, type-safe request-response |
| Expression serialization | Store SQL, recompile on restart | Simple, durable, avoids brittle AST serialization |

All research items are now resolved. **Phase 0 complete** - proceed to Phase 1 (Design & Contracts).
