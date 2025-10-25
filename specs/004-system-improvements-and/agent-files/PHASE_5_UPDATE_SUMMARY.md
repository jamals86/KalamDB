# Phase 5 Update Summary: Tokio-Based Job Management

**Date**: October 22, 2025  
**Branch**: `004-system-improvements-and`  
**User Story**: US2 - Automatic Table Flushing with Job Management

## Changes Made

Phase 5 (User Story 2) has been updated from an actor-based implementation to a **Tokio-based job registry with generic JobManager trait interface**. This provides immediate functionality while maintaining flexibility for future actor migration.

### Key Design Decisions

1. **JobManager Trait**: Generic interface for job lifecycle management (start, cancel, get_status)
2. **TokioJobManager Implementation**: Uses `HashMap<JobId, JoinHandle>` for job tracking
3. **Job Cancellation**: Implements `JoinHandle::abort()` for lightweight task cancellation
4. **KILL JOB Command**: SQL command for operational control over long-running jobs
5. **Future-Proof**: Interface designed to support actor-based implementation later

### Updated Documents

#### 1. spec.md
**Location**: `specs/004-system-improvements-and/spec.md`

**Changes**:
- Updated User Story 2 title to "Automatic Table Flushing with Scheduled Jobs and Job Management"
- Added acceptance scenarios #6 and #7 for KILL JOB functionality
- Added integration tests #8, #9, #10 for job cancellation
- Updated FR-022 to specify Tokio-based job registry instead of actor model
- Added FR-022a for JobManager trait requirement
- Added FR-022b for future actor migration design
- Added FR-024, FR-025, FR-026 for KILL JOB command requirements

#### 2. plan.md
**Location**: `specs/004-system-improvements-and/plan.md`

**Changes**:
- Updated summary to mention "Tokio-based job scheduling and cancellation"
- Changed technical unknown #7 from "Flush job actor communication" to "JobManager trait methods"
- Added performance note about lightweight JoinHandle::abort()
- Updated transparency section to mention JobManager and KILL JOB command
- Updated ADR-005 description to "Tokio-based job management with JobManager trait"

#### 3. tasks.md
**Location**: `specs/004-system-improvements-and/tasks.md`

**Changes**:
- Updated Phase 5 title to include "Job Management"
- Updated goal to mention "Tokio-based job cancellation with generic JobManager interface"
- Added 3 new integration tests (T144a, T144b, T144c) for KILL JOB functionality
- Replaced T150 (actor implementation) with TokioJobManager implementation
- Added T150a, T150b for JobManager trait and interface design
- Added T158a, T158b, T158c for KILL JOB SQL command implementation
- Updated documentation tasks to reflect Tokio approach
- Updated checkpoint to mention "job cancellation via KILL JOB command"

#### 4. research.md
**Location**: `specs/004-system-improvements-and/research.md`

**Changes**:
- Replaced Research Item 7 from "Flush Job Actor Communication Protocol" to "Job Management with Tokio-Based Registry"
- Added complete code examples for JobManager trait
- Added TokioJobManager implementation with RwLock-protected HashMap
- Added KILL JOB SQL command integration example
- Updated rationale to explain trait-based abstraction approach
- Added "Migration Path to Actor Model" section with example ActorJobManager
- Updated alternatives considered to explain why actor model is deferred

## Implementation Approach

### Phase 5 Immediate Implementation (Tokio-Based)

```rust
// Generic interface
pub trait JobManager: Send + Sync {
    async fn start_job(&self, job_type: JobType, params: JobParams) -> Result<JobId>;
    async fn cancel_job(&self, job_id: JobId) -> Result<()>;
    async fn get_job_status(&self, job_id: JobId) -> Result<JobStatus>;
    async fn list_active_jobs(&self) -> Result<Vec<JobInfo>>;
}

// Initial implementation
pub struct TokioJobManager {
    jobs: Arc<RwLock<HashMap<JobId, JoinHandle<Result<()>>>>>,
    status_store: Arc<dyn JobStatusStore>,
}
```

### Future Migration Path (Actor-Based)

```rust
// Future implementation - same trait, different backend
pub struct ActorJobManager {
    actor_tx: mpsc::Sender<ActorMessage>,
}

impl JobManager for ActorJobManager {
    // Same interface, actor-based implementation
}
```

## Benefits of This Approach

1. **Simplicity**: Tokio JoinHandles are lightweight and well-understood
2. **Immediate Functionality**: No additional dependencies or complex setup
3. **Operational Control**: KILL JOB command provides admin control
4. **Future Flexibility**: JobManager trait allows seamless migration to actors
5. **Testability**: Easier to test Tokio-based approach than actor systems
6. **Performance**: Minimal overhead compared to message-passing actors

## SQL Command Example

```sql
-- Start a manual flush (creates a job)
FLUSH TABLE my_namespace.my_table;

-- Query job status
SELECT * FROM system.jobs WHERE job_id = 'abc-123';

-- Cancel a long-running job
KILL JOB 'abc-123';
```

## Task Breakdown

### New Tasks Added
- **T144a**: Test KILL JOB cancellation
- **T144b**: Test KILL JOB with invalid job_id
- **T144c**: Test concurrent job management
- **T146a**: Create JobManager trait
- **T150a**: Implement JobManager trait methods
- **T150b**: Ensure interface supports future actor migration
- **T158a**: Implement KILL JOB SQL command
- **T158b**: Add KILL JOB parsing to SQL executor
- **T158c**: Update job status on cancellation
- **T161a**: Document TokioJobManager implementation
- **T162a**: Update ADR-005 with trait abstraction rationale

### Modified Tasks
- **T146**: Changed from "FlushJob and actor logic" to "FlushJob implementation"
- **T150**: Changed from "FlushJob actor with message-based protocol" to "TokioJobManager with HashMap<JobId, JoinHandle>"
- **T161**: Changed from "actor protocol" to "JobManager trait design rationale"
- **T162**: Changed from "actor model" to "Tokio-based job management and future actor migration path"

## Next Steps

1. Implement JobManager trait in `/backend/crates/kalamdb-core/src/job_manager.rs`
2. Implement TokioJobManager in `/backend/crates/kalamdb-core/src/job_manager.rs`
3. Update FlushScheduler to use JobManager trait
4. Implement KILL JOB SQL command in `/backend/crates/kalamdb-sql/src/job_commands.rs`
5. Add integration tests for job cancellation
6. Document design in ADR-005

## Compatibility Notes

- **No Breaking Changes**: This is a new feature, not a modification of existing functionality
- **Interface Stability**: JobManager trait is designed to be stable across implementations
- **Migration Path**: When ready to move to actors, only TokioJobManager needs replacement
- **Testing**: Both implementations can be tested against same JobManager trait interface

## References

- **Original Spec**: specs/004-system-improvements-and/spec.md (User Story 2)
- **Implementation Tasks**: specs/004-system-improvements-and/tasks.md (Phase 5)
- **Design Research**: specs/004-system-improvements-and/research.md (Research Item 7)
- **Architecture Decision**: ADR-005 (to be created)
