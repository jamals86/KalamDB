<!--
Sync Impact Report:
- Version change: 2.0.0 → 2.1.0
- Modified principles: None
- Added principles: 
  - VIII. Self-Documenting Code (comprehensive inline documentation and architectural explanations)
- Added sections: 
  - New principle VIII mandating inline comments, class-level documentation, and architectural explanations
  - Enhanced Documentation section in Technical Standards requiring inline comments and architecture documentation
- Removed sections: None
- Templates requiring updates: 
  - ✅ Constitution updated to v2.1.0
  - ✅ Plan template updated with documentation requirements checklist
  - ✅ Tasks template updated with documentation tasks for each phase
  - ✅ Spec template updated with documentation success criteria
- Follow-up TODOs: 
  - Review existing codebase for documentation compliance (see kalamdb-core/src/lib.rs, models/, storage/)
  - Update code review checklist to verify documentation requirements
  - Consider adding automated linting for missing rustdoc comments (cargo doc --no-deps)
  - Add documentation quality checks to CI/CD pipeline
-->

# KalamDb Constitution

## Mission

KalamDb is a lightweight Rust-based database for real-time chat, message history, and AI conversation storage.

Its mission is to make user-centric data durable, queryable, and streamable — empowering developers to build systems where data moves like a stream but persists like a lake.

## Core Principles

### I. Simplicity First

Design for clarity. Code paths MUST be direct and easy to follow.
Over-engineering MUST be avoided in favor of straightforward, maintainable solutions.
Every abstraction MUST justify its complexity with concrete benefits.
Implementation choices MUST prioritize developer understanding and debuggability.

**Rationale**: Simplicity reduces bugs, eases maintenance, and accelerates development. 
Complex systems are harder to reason about, test, and extend.

### II. Performance by Design

Performance targets are non-negotiable requirements, not aspirational goals:
- Writes MUST complete in under 1ms (RocksDB path)
- Storage MUST use compressed, analytics-ready format (Parquet)
- Queries MUST be executed via standard SQL (DataFusion)

All performance-critical paths MUST be benchmarked and continuously monitored.
Performance regressions MUST be caught in CI before merge.

**Rationale**: KalamDb is designed for real-time chat where latency directly impacts user experience.
Predictable performance is a core feature, not an optimization.

### III. Data Ownership

Every user's data MUST live in isolated partitions with clear boundaries.
Each user MUST have their own logical table (userId.messages) backed by dedicated storage partitions.
Data MUST be easily replicated, exported, and migrated without vendor lock-in.
Privacy and encryption MUST be first-class features, not afterthoughts.
Users MUST have complete control over their data lifecycle.

**Table-Per-User Architecture**: KalamDB uses a table-per-user design where each user's messages
are stored in isolated partitions (`{userId}/batch-*.parquet`). This enables:
- **Massive Scalability**: Millions of concurrent users can maintain real-time subscriptions to their own tables
  without performance degradation from shared table triggers or locks
- **Simple Real-time Notifications**: File-level monitoring per user replaces complex database triggers
  on shared tables containing billions of rows
- **O(1) Subscription Complexity**: Each user's subscription monitors only their partition, 
  independent of total user count
- **Horizontal Scaling**: Adding users doesn't impact existing user performance
- **Efficient Queries**: Direct partition access eliminates userId filtering overhead

**Rationale**: User-centric data ownership builds trust and enables compliance with privacy regulations.
Isolated partitions enable horizontal scaling and data sovereignty. The table-per-user model is a 
fundamental architectural decision that differentiates KalamDB from traditional shared-table databases,
enabling real-time chat at unprecedented scale.

### IV. Zero-Copy Efficiency

Memory allocations MUST be minimized through zero-copy techniques.
Arrow IPC and Parquet formats MUST be used to reduce deserialization overhead.
Data transformations MUST avoid unnecessary copies wherever possible.
Memory usage patterns MUST be predictable and measurable.

**Rationale**: Chat and conversation data can be voluminous. Zero-copy techniques directly translate
to lower latency, reduced memory pressure, and better resource utilization.

### V. Open & Extensible

All components MUST be modular and independently usable.
KalamDb MUST be embeddable as both a server and a library inside applications.
Interfaces MUST be well-defined to support custom implementations.
Extension points MUST be documented and stable.

**Rationale**: Flexibility in deployment models (embedded vs. server) maximizes adoption.
Modular architecture enables incremental adoption and custom integrations.

### VI. Transparency

Every operation (change, flush, subscription) MUST be observable.
Structured logs MUST include request IDs, timing, and resource metrics.
Events MUST be emitted for state transitions and data flow.
Internal state MUST be inspectable for debugging and monitoring.

**Rationale**: Real-time systems require deep observability for troubleshooting performance issues,
understanding data flow, and maintaining system health.

### VII. Secure by Default

Authentication MUST use industry-standard JWT tokens.
Tenant isolation MUST be enforced at the storage and query layer.
Message content and metadata MAY be encrypted using AEAD (Authenticated Encryption with Associated Data).
Security MUST be enabled by default, with opt-out requiring explicit configuration.

**Rationale**: Chat and AI conversations contain sensitive data. Security cannot be optional.
Defense-in-depth with multiple security layers protects against various threat vectors.

### VIII. Self-Documenting Code

Every module, struct, enum, function, and significant code block MUST include clear documentation
explaining its purpose, behavior, and role in the overall architecture.

Code documentation requirements:
- **Module-level documentation**: Each module MUST have a rustdoc comment explaining its purpose,
  primary responsibilities, and how it fits into the broader system architecture
- **Type documentation**: Every public struct, enum, and trait MUST have rustdoc comments describing:
  - What the type represents in the domain model
  - Its key responsibilities and invariants
  - Usage examples for non-trivial types
- **Function documentation**: Every public function MUST document:
  - Purpose and behavior
  - Parameter meanings and constraints
  - Return value semantics
  - Error conditions and edge cases
  - Examples for complex APIs
- **Inline comments**: Complex algorithms, non-obvious optimizations, architectural decisions,
  and business logic MUST have inline comments explaining the "why" behind the implementation
- **Architecture explanations**: Code that implements key architectural patterns
  (table-per-user partitioning, zero-copy data flow, real-time notification, etc.)
  MUST include comments explaining how the pattern works and why it was chosen

**Rationale**: Code is read far more often than it is written. Self-documenting code with
comprehensive comments accelerates onboarding, reduces bugs from misunderstanding, enables
confident refactoring, and preserves architectural knowledge. In a database system where
performance and correctness are critical, understanding the "why" behind implementation
decisions is essential for maintainability and evolution.

## Technical Standards

**Language & Runtime**: Rust 1.75+ with stable toolchain only. No nightly features in production code.

**Core Technology Stack**:
- **Write Path**: RocksDB for sub-millisecond write latency
- **Storage Format**: Apache Parquet for compressed, columnar storage
- **Query Engine**: DataFusion for SQL query execution
- **Data Format**: Apache Arrow for zero-copy in-memory representation

**Dependencies**: Minimize external dependencies; prefer established crates in the Rust ecosystem (tokio, serde, arrow, parquet, datafusion, rocksdb).

**Error Handling**: Use Result<T, E> types throughout. No panics in library code except for invariant violations.

**Documentation**: 
- All public APIs MUST have comprehensive rustdoc comments with examples demonstrating real-world usage patterns
- Module-level documentation MUST explain the module's purpose and architectural role
- Complex algorithms and architectural patterns MUST include inline comments explaining implementation rationale
- Non-obvious code MUST be explained with comments focusing on "why" rather than "what"
- Key architectural decisions (table-per-user partitioning, zero-copy flows, real-time notifications)
  MUST be documented both in code comments and in separate Architecture Decision Records (ADRs)
- Examples MUST be realistic and demonstrate actual use cases, not toy scenarios

**Performance**: Benchmark critical paths with criterion.rs. Write latency MUST stay under 1ms target. Performance regressions >5% require explicit justification.

**Security**: 
- JWT-based authentication MUST be implemented for all network endpoints
- Tenant isolation MUST be enforced at data partition boundaries
- AEAD encryption MUST be available for sensitive message content
- Input validation and sanitization are mandatory

## Development Workflow

**Code Review**: All changes require review by at least one maintainer familiar with the affected subsystem.

**Testing Gates**: 
- CI MUST pass all unit tests, integration tests, and security scans before merge
- Property-based tests MUST be used for data transformation and query logic
- Performance benchmarks MUST not regress beyond acceptable thresholds

**Performance Gates**: 
- Write latency benchmarks MUST stay under 1ms (p99)
- Memory usage MUST be tracked for common workloads
- Query performance MUST be validated against representative datasets

**Documentation**: 
- Public API changes MUST include updated rustdoc and examples
- All new code MUST include appropriate inline comments explaining complex logic and architectural patterns
- Architecture decisions MUST be documented in ADRs (Architecture Decision Records)
- Performance characteristics MUST be documented for public APIs
- Code reviews MUST verify that documentation requirements from Principle VIII are met

**Security Review**: Changes affecting authentication, tenant isolation, encryption, or data access boundaries require security review.

## Governance

This constitution supersedes all other development practices and coding standards.

All pull requests and code reviews MUST verify compliance with these principles.

Amendments require consensus among project maintainers and a documented migration plan.

Complexity must be justified against these principles; simplicity is preferred when principles are satisfied.

When principles conflict, the order of precedence is:
1. Secure by Default (security cannot be compromised)
2. Data Ownership (user data protection is paramount)
3. Performance by Design (real-time requirements are core)
4. Simplicity First (avoid complexity when possible)
5. Self-Documenting Code (maintainability and knowledge preservation)
6. Transparency, Zero-Copy Efficiency, Open & Extensible (optimize within constraints)

For runtime development guidance, refer to project documentation and established patterns in `/docs`.

**Version**: 2.1.0 | **Ratified**: 2025-10-13 | **Last Amended**: 2025-10-14