````markdown
# File Datatype + File Uploads Spec

Date: 2026-01-24
Owner: KalamDB

## Goals
- Add a new `FILE` datatype usable in SQL insert/update payloads.
- Support multipart SQL endpoint uploads with file references bound to `FILE` columns.
- Persist file blobs in storage with deterministic layout and safe naming.
- Represent file values as a JSON object (`FileRef`) stored in batches/segments.
- Enable authenticated download by `table_id` / `row_id` / `file_id`.
- Ensure deletes and updates clean up file objects (best-effort async cleanup).

## Non-Goals
- Inline/embedded file storage inside RocksDB or Parquet.
- Cross-table file sharing or deduplication (future work).
- File search/content indexing.

## Current State (Observed)
- Tables store structured values in RocksDB and Parquet segments.
- SQL endpoint accepts JSON payloads; multipart file upload is not supported yet.
- Table folders exist at `user/tableid/<user>/` for user tables and shared tables use a shared table folder (no file subfolders today).

## Proposed Data Model
### `FILE` datatype
- Logical column type `FILE` in SQL schema.
- Stored value is a JSON object `FileRef`.

### `FileRef` (JSON)
```json
{
  "id": "<unique-id>",
  "sub": "f0001",
  "name": "original-filename.ext",
  "size": 12345,
  "mime": "image/png",
  "sha256": "<hex>"
}
```
```
Notes:
- Required fields for v1: `id`, `sub`, `name`, `size`, `mime`, `sha256`.
- `sub` is the logical folder name within the table folder.
- `name` is the original filename (not the stored object name).

## Storage Layout
### User tables
```
user/tableid/<user>/
  f0001/
    <unique-id>-<sanitized-name>.<ext>
  f0002/
    ...
```

### Shared tables
```
shared/tableid/<table>/
  shard-1/
    f0001/
      <unique-id>-<sanitized-name>.<ext>
  shard-2/
    f0001/
      ...
```

Notes:
- `f000x` is a subfolder created as file count exceeds limit (e.g., 5k).
- This limit can be configured by the server.toml config.

## File Naming
- Stored filename: `<unique-id>-<sanitized-name>.<ext>`
- `sanitized-name` is ASCII lower-case a-z0-9 with dashes; no spaces and limited to 50 chars max.
- `ext` is the original file extension (lowercase), or `bin` if none
- If original filename is non-ASCII or empty, store as `<unique-id>.<ext>`.
- `unique-id` can be snowflake or ULID; must be globally unique.

## File Folder Rotation
- Each table (and shard for shared tables) uses subfolders `f0001`, `f0002`, …
- Rotate when file count in current subfolder exceeds limit (e.g., 5000).
- Keep a fast counter in the table manifest (segment metadata) to avoid scanning:
  - 'files': {
        `subfolder` (index)
        `count` (number of files in current subfolder)
    }
- Increment subfolder index and reset count when limit is reached.

## SQL & API Surface
### SQL Parsing
- Add `FILE` datatype to SQL schema parser and type system.
- Allow binding file references using a placeholder function:
  - `INSERT INTO chat.messages (...) VALUES (..., FILE("avatar"), FILE("invoice_pdf"))`
- During SQL request parsing, extract required file part names (e.g., `avatar`, `invoice_pdf`).
- Also sql arguments/parameters should work for this.
- AS USER which is impersonation should also work as normal, and the files should be in the impersonated users table.

### Upload SQL Endpoint (multipart)
- Endpoint: existing SQL endpoint supports multipart/form-data.
- Required parts:
  - `sql`: the SQL string
  - `file:<name>` parts matching placeholders in SQL
- Validation:
  - Multipart must contain exactly the required `file:<name>` parts.
  - Reject extra file parts.

### Download Endpoint
- New endpoint for file download:
  - `GET /v1/files/{table_id}/{subfolder}/{file_id}`
- Requires JWT authorization and permissions for that table.
- Doesnt need to read the row itself the file will be directly accessible if you have table access.
- This will work for both users tables and shared tables.
- For impersonation we can pass ?user_id=... as a query param to access user tables instead of the current user.

## End-to-End Flow (Insert/Update with Files)
1. Parse SQL, extract required file names: `avatar`, `invoice_pdf`, …
2. Validate multipart contains exactly those `file:<name>` parts.
3. For each file:
   - Stream upload to temporary directory: `/data/tmp/<requestid>-<user>/`.
   - Compute `size`, `sha256`, `mime`.
   - Enforce limits (size, allowed mime, total files per request, etc.).
4. Begin DB transaction.
   - For UPDATE: lock target row(s) and read old `FileRef` values.
5. Finalize files:
   - Move/copy from staging → final key (unique, never overwrite).
   - Resolve `fxxxx` subfolder using manifest counters same structure with the manifest service we already have.
6. Execute SQL with `FILE(name)` placeholders replaced by `FileRef` JSON values. also support parameters/arguments in the query
7. Commit DB transaction.
8. Cleanup:
   - Delete staging files.
   - Enqueue deletion of old `FileRef` objects (async) if updated/replaced.

### Failure Handling
- If any step fails:
  - Rollback DB transaction if started.
  - Delete staging uploads.
  - If finalization completed but DB failed, delete finalized uploads (best-effort).

## Delete Semantics
- Deleting a row deletes any referenced files (best-effort async cleanup).
- Updating a `FILE` column deletes the previous file after successful commit.

## Limits & Validation (Suggested Defaults)
- Max file size: configurable (e.g., 25 MB default).
- Max files per request: configurable (e.g., 20).
- Allowed mime types: configurable allowlist.
- Reject duplicate `file:<name>` parts.
- The max file upload size can be configured inside server.toml

## Storage & Responsibility Boundaries
- File storage logic (naming, folders, move/copy, delete) must live in `kalamdb-filestore`.
- `kalamdb-core` orchestrates SQL binding, transaction flow, and manifest counters.
- `kalamdb-api` handles multipart parsing, auth, and routing.
- `kalamdb-commons` provides `FileRef` model/type and `FILE` datatype enum.
- Make sure using the same logic for getting the table folder path as existing storage logic.

## Manifest Updates
- Extend table manifest to include:
    "files": {
        "subfolder": 1,
        "count": 3456
    }
- Update these counters in the same transaction as file finalize and row insert/update.
- If not file columns in the table files should be null or absent.

## Observability
- Log file finalize and cleanup events with `table_id`, `row_id`, `file_id`.
- Track metrics for file upload size, counts, failures.

## Validation Checklist
- [ ] `FILE` datatype parsed and available in schema definitions.
- [ ] Multipart SQL accepts `file:<name>` matching `FILE(name)` placeholders.
- [ ] Staging → finalization flow works with cleanup on failures.
- [ ] File subfolder rotation when limit reached.
- [ ] Row delete/update removes old file references asynchronously.
- [ ] Download endpoint enforces JWT and permissions.

## Risks & Mitigations
- **Partial failure after finalize**: best-effort cleanup and orphan reaper job (future).
- **Manifest counter drift**: periodic reconcile task to resync counts (future).
- **Large uploads blocking**: use streaming and backpressure; apply size limits early.

````
