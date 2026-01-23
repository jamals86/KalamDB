# Storage & Flush Lifecycle Test Scenarios [CATEGORY:STORAGE]

This document covers the movement of data from the hot tier (memory/RocksDB) to the cold tier (Parquet).

## 1. Flush Triggers & Policies
**Goal**: Verify that data is flushed according to configured policies.
- **Scenario A**: Row-based policy (`rows:50`).
  1. Insert 50 rows into a USER table.
  2. Verify a background job is triggered.
  3. Verify data is moved to Parquet files.
- **Scenario B**: Manual Flush.
  1. INSERT data but don't reach threshold.
  2. Execute `STORAGE FLUSH TABLE <name>`.
  3. Verify immediate job execution.
- **Agent Friendly**: Check `backend/tests/testserver/flush/test_flush_policy_verification_http.rs`.

## 2. Multi-Storage Routing
**Goal**: Verify the `STORAGE_ID` routing logic.
- **Steps**:
  1. Configure two storage backends: `fast_ssd` (local) and `s3_archive` (local fake/minio/mock).
  2. Create Table A on `fast_ssd` and Table B on `s3_archive`.
  3. Flush both.
  4. Verify files exist in the distinct physical directories associated with each Storage ID.
- **Agent Friendly**: Check `backend/tests/scenarios/scenario_11_multi_storage.rs`.

## 3. Job Resilience & Recovery
**Goal**: Handle failures in the flush pipeline.
- **Scenario**:
  1. Trigger flush but inject a "Disk Full" or "Network Error" (via mock).
  2. Verify Job state becomes `FAILED`.
  3. Fix the "error" and trigger a retry via `system.jobs`.
  4. Verify data integrity and no duplications in Parquet.
- **Agent Friendly**: Check `backend/tests/scenarios/scenario_06_jobs.rs`.

## 4. Cold Tier Reading (Manifest & Parquet)
**Goal**: Seamless union of Hot + Cold data.
- **Steps**:
  1. Insert data, flush.
  2. Insert new data (Hot).
  3. Run a query that spans both.
  4. Verify DataFusion correctly unions the RocksDB scan and Parquet scan.
- **Agent Friendly**: Check `backend/tests/testserver/manifest/test_manifest_flush_http_v2.rs`.

## Coverage Analysis
- **Duplicates**: Manifest update logic is tested in multiple places (Scenario 06, `test_manifest_flush_http_v2.rs`).
- **Gaps**: True S3/Object storage E2E tests are missing (running against real/localstack S3). Compaction of Parquet segments is a placeholder.
