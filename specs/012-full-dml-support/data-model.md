# Data Model: Full DML Support

## RecordVersion
- Fields: `_id` (SnowflakeId), `_updated` (timestamp ns), `_deleted` (bool), `payload` (table-defined columns), `version_source` (enum: FastStorage | ParquetBatch)
- Relationships: Linked to table via `TableId`; participates in VersionResolution across storage layers.
- Validation: `_updated` must be strictly monotonic per `_id`; `_deleted` defaults to false; `_id` is system managed.
- State transitions: INSERT → Active; UPDATE → New RecordVersion appended, previous remains historical; DELETE → `_deleted = true` version appended.

## VersionResolution
- Fields: `table_id`, `record_id`, `resolved_version` (RecordVersion), `source_layers` (Vec<VersionSource>).
- Relationships: Aggregates RecordVersion entries from RocksDB and Parquet via Manifest metadata.
- Validation: Chooses MAX(`_updated`); ties resolved by source priority (FastStorage > Parquet).
- State transitions: Recomputed per query; no persistence.

## ManifestFile
- Fields: `table_id`, `scope` (user_id or shared), `version`, `generated_at`, `max_batch`, `batches` (Vec<BatchFileEntry>).
- Relationships: Stored in `/data/{namespace}/{table}/{scope}/manifest.json`; cached in ManifestCache.
- Validation: `max_batch` equals max(`BatchFileEntry.batch_number`); JSON schema versioned.
- State transitions: `Created` (initial flush), `Updated` (each flush), `Rebuilt` (manifest recovery).

## BatchFileEntry
- Fields: `batch_number`, `file_path`, `min_updated`, `max_updated`, `column_min_max` (Map<ColumnName, (Value, Value)>), `row_count`, `size_bytes`, `schema_version`, `status` (enum: Active | Compacting | Archived).
- Relationships: Part of ManifestFile; corresponds to Parquet file on storage backend.
- Validation: `file_path` must match naming `batch-{number}.parquet`; timestamps ordered; `status` transitions follow flush/compaction lifecycle.
- State transitions: Active → Compacting (during compaction job) → Archived (post-compaction retention).

## ManifestCacheEntry
- Fields: `cache_key` (`{namespace}:{table}:{scope}`), `manifest_json`, `etag`, `last_refreshed`, `sync_state` (InSync | Stale | Error), `source_path`.
- Relationships: Persisted in RocksDB `manifest_cache` CF; mirrored in-memory (DashMap) with `last_accessed` metadata.
- Validation: `manifest_json` must deserialize to ManifestFile schema; TTL enforced via config.
- State transitions: Cached → Refreshed (TTL or stale detection) → Evicted (ManifestEvictionJob).

## ManifestCacheConfig
- Fields: `ttl_seconds`, `eviction_interval_seconds`, `max_entries`, `last_accessed_memory_window`.
- Relationships: Loaded from `AppContext.config().manifest_cache` and injected into ManifestCacheService & jobs.
- Validation: All durations > 0; `max_entries` > 0.
- State transitions: Static per deployment; adjustments require restart.

## ManifestService
- Fields: `store` (ManifestCacheStore), `storage_backend` (S3/local), `hot_cache` (DashMap), `config` (ManifestCacheConfig).
- Relationships: Used by flush executors, query planner, eviction job; accessed via SchemaRegistry.
- Validation: Ensures atomic write-through updates on flush and rebuilds manifest on corruption detection.
- State transitions: Provides operations `get_or_load`, `update_after_flush`, `rebuild`, `evict`.

## ManifestEvictionJobParameters
- Fields: `namespace_filter` (Option<String>), `max_entries`, `ttl_seconds`, `dry_run` (bool).
- Relationships: Typed parameters for ManifestEvictionJob executor within UnifiedJobManager.
- Validation: `max_entries` and `ttl_seconds` default from config when absent; `dry_run` toggles write mode.
- State transitions: Parameters remain immutable per job execution.

## ImpersonationContext
- Fields: `actor_user_id`, `actor_role`, `subject_user_id`, `subject_role`, `session_id`, `origin` (SQL | API).
- Relationships: Constructed by SQL handler when parsing AS USER; passed to DML execution and audit logging.
- Validation: `actor_role` must be Service or Admin for AS USER usage; `subject_user_id` must exist and not be soft-deleted.
- State transitions: Created per statement; disposed after execution.

## SystemColumnsService
- Fields: `snowflake_generator`, `time_provider`, `config`, `schema_registry` references.
- Relationships: Called by DDL, DML handlers, query planner to manage `_id`, `_updated`, `_deleted`.
- Validation: Guards against manual `_id` assignments, enforces `_updated` monotonicity, ensures `_deleted` filter applied.
- State transitions: Maintains per-table caches (optional) for schema augmentation; stateless otherwise.

## SnowflakeGenerator
- Fields: `node_id_hash` (u16), `last_timestamp`, `sequence` (u16).
- Relationships: Used by SystemColumnsService; reads node identifier from AppContext config.
- Validation: Enforces timestamp monotonicity (waits or bumps 1 ns) and caps sequence per tick.
- State transitions: Sequence resets when timestamp advances; on backward clock drift, waits until safe time.

## TypedJobParameterEnvelope
- Fields: `job_type`, `raw_json`, `typed_params` (generic), `schema_version`.
- Relationships: Stored in system.jobs; deserialized for each executor via serde.
- Validation: JSON must conform to executor struct; schema version ensures forward/backward compatibility.
- State transitions: Stored → Deserialized → Validated → Consumed by executor.
