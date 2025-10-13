<!--
Sync Impact Report:
- Version change: 1.0.0 → 2.0.0
- Modified principles: Complete redesign for KalamDb real-time chat/conversation database
  - I. Database-First Design → I. Simplicity First
  - II. Rust Performance & Safety → II. Performance by Design (specific targets: <1ms writes, Parquet compression, SQL queries)
  - III. Test-Driven Development → III. Data Ownership (user-centric partitions, privacy, encryption)
  - IV. API Contract Stability → IV. Zero-Copy Efficiency (Arrow IPC, Parquet optimization)
  - V. Observability & Debugging → V. Open & Extensible (modular, embeddable architecture)
  - Added: VI. Transparency (observable operations via structured logs/events)
  - Added: VII. Secure by Default (JWT, tenant isolation, AEAD encryption)
- Added sections: Mission statement defining real-time chat/AI conversation storage focus
- Removed sections: Generic database principles replaced with KalamDb-specific requirements
- Templates requiring updates: 
  - ✅ Constitution updated to v2.0.0
  - ⚠ Plan template may need KalamDb-specific technical context (RocksDB, DataFusion, Arrow, Parquet)
  - ⚠ Tasks template may need KalamDb-specific task categories (streaming, partitioning, encryption)
- Follow-up TODOs: Consider updating plan-template.md to include KalamDb technology stack
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
Data MUST be easily replicated, exported, and migrated without vendor lock-in.
Privacy and encryption MUST be first-class features, not afterthoughts.
Users MUST have complete control over their data lifecycle.

**Rationale**: User-centric data ownership builds trust and enables compliance with privacy regulations.
Isolated partitions enable horizontal scaling and data sovereignty.

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

## Technical Standards

**Language & Runtime**: Rust 1.75+ with stable toolchain only. No nightly features in production code.

**Core Technology Stack**:
- **Write Path**: RocksDB for sub-millisecond write latency
- **Storage Format**: Apache Parquet for compressed, columnar storage
- **Query Engine**: DataFusion for SQL query execution
- **Data Format**: Apache Arrow for zero-copy in-memory representation

**Dependencies**: Minimize external dependencies; prefer established crates in the Rust ecosystem (tokio, serde, arrow, parquet, datafusion, rocksdb).

**Error Handling**: Use Result<T, E> types throughout. No panics in library code except for invariant violations.

**Documentation**: All public APIs MUST have rustdoc comments with examples demonstrating real-world usage patterns.

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
- Architecture decisions MUST be documented in ADRs (Architecture Decision Records)
- Performance characteristics MUST be documented for public APIs

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
5. Transparency, Zero-Copy Efficiency, Open & Extensible (optimize within constraints)

For runtime development guidance, refer to project documentation and established patterns in `/docs`.

**Version**: 2.0.0 | **Ratified**: 2025-10-13 | **Last Amended**: 2025-10-13