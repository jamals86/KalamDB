# Contract: Unified JobManager (009)

Purpose: Single component to create, schedule, execute, and monitor jobs with typed short IDs, richer statuses, idempotency, retry/backoff, and dedicated logging.

## API Surface (Rust)

- create_job(job_type: JobType, params: Value, idempotency_key: Option<String>, options: JobOptions) -> Result<JobId>
  - Side effects: persist New job to system.jobs; enqueue
  - Errors: IdempotentConflict { existing_job_id, status }

- cancel_job(job_id: &JobId) -> Result<()> 
  - Side effects: send cancellation signal; transition to Cancelled if Running/Queued

- get_job(job_id: &JobId) -> Result<Job>
- list_jobs(filter: JobFilter) -> Result<Vec<Job>>

- run_loop(max_concurrent: usize) -> Result<()>  
  - Behavior: continuously poll and execute ready jobs with backoff and idempotency enforcement

- poll_next() -> Result<Option<Job>>  
  - Behavior: fetch next queued job honoring idempotency and concurrency quotas

## Types

- JobOptions { max_retries: u8, queue: Option<String>, priority: Option<i32> }
- JobFilter { job_type: Option<JobType>, status: Option<JobStatus>, since: Option<i64> }
- Errors: IdempotentConflict, NotFound, StorageError, InternalError

## Logging Contract

- All log lines written to jobs.log prefixed by `[<JobId>]` and include level
- Minimum events:
  - queued, started, progress (optional), retry(attempt/limit, delay_ms), completed, failed(error), cancelled

## Persistence Contract

- Writes/updates occur via SystemTablesRegistry provider for system.jobs
- Fields updated on each transition:
  - status, message, exception_trace, retry_count, updated_at
  - started_at on transition to Running; finished_at on terminal states

## Concurrency & Idempotency

- Idempotency key uniqueness enforced only among active states {New, Queued, Running, Retrying}
- Max concurrency enforced at JobManager level; executor per job type is pluggable

## Success Criteria

- All acceptance scenarios (1–33) in spec are satisfied with automated tests
