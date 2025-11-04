# Phase 1: Data Model and Contracts (009)

This document defines the data structures and schema changes for the Core Architecture Refactor, with focus on the Unified Job Management System.

## Job System Data Model

- JobId (typed short ID)
  - Format: `<PREFIX>-<base62>` where prefix encodes job type
  - Prefix map:
    - FL (Flush), CL (Cleanup), RT (Retention), SE (StreamEviction), UC (UserCleanup), CO (Compact), BK (Backup), RS (Restore)
  - Constraints: 2-letter uppercase prefix; base62 length 6–10; total length ≤ 16

- JobType (enum in commons)
  - Variants: Flush, Cleanup, Retention, StreamEviction, UserCleanup, Compact, Backup, Restore, Unknown
  - Provide `short_prefix()` -> &'static str

- JobStatus (enum in commons)
  - Variants: New, Queued, Running, Retrying, Completed, Failed, Cancelled
  - Rationale: Adds New, Queued, Retrying to existing set for better lifecycle visibility

- Job record (system.jobs)
  - id: JobId (string, PK)
  - job_type: JobType (enum as string)
  - status: JobStatus (enum as string)
  - message: Option<String> (success or error summary)
  - exception_trace: Option<String> (full stack trace on failures)
  - parameters: serde_json::Value (object; not array)
  - idempotency_key: Option<String>
  - retry_count: u8 (default 0)
  - max_retries: u8 (default 3; configurable)
  - created_at: i64 (ms epoch)
  - updated_at: i64 (ms epoch)
  - started_at: Option<i64>
  - finished_at: Option<i64>
  - node_id: Option<NodeId> (executor node)
  - queue: Option<String> (future use)
  - priority: Option<i32> (future use)

### Backward Compatibility Mapping

- Replace legacy `result` and `error_message` fields with `message`
  - Read-time compatibility: if querying legacy fields, map to `message`
- Parameters format migration
  - If legacy JSON array is detected, migrate to object by index: `["a","b"]` -> `{ "arg0": "a", "arg1": "b" }`
- Status mapping
  - Legacy {Running, Completed, Failed, Cancelled} remain; new states extend lifecycle; no breaking changes

## System Tables Impact

- commons models remain single SoT; update only in `kalamdb-commons`:
  - Add JobId newtype with generator (prefix + base62)
  - Extend JobType with `short_prefix()` and add missing variants
  - Extend JobStatus with New, Queued, Retrying
  - Update User/Table/Namespace unaffected by this feature

- kalamdb-core:
  - JobManager (new unified component) creates JobId, writes system.jobs via SystemTablesRegistry
  - jobs.log: prefix every line with `[<JobId>]` and level

## SqlExecutor Handlers Contracts (overview)

- Handlers operate on (&SessionContext, &str sql, &ExecutionContext)
- Lookup table definitions via SchemaRegistry
- Dispatch to providers/stores via AppContext
- Invalidate SchemaRegistry cache on ALTER/DROP

## Migration Notes

- Data migration for system.jobs may be lazy (on read) to avoid costly backfill
- Ensure CLI and SDKs display `message` and ignore removed fields
