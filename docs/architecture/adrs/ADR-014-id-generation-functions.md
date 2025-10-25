# ADR-014: ID Generation Functions - SNOWFLAKE_ID, UUID_V7, ULID

**Status**: Accepted  
**Date**: 2025-10-25  
**Deciders**: KalamDB Core Team  
**Technical Story**: US15 - SQL Functions for Unique ID Generation

## Context and Problem Statement

Modern applications need globally unique, time-sortable identifiers for:
- Primary keys in distributed systems
- Correlation IDs for request tracing  
- Event IDs in event-sourcing systems
- Session IDs and tracking tokens

We need to provide multiple ID generation strategies, each optimized for different use cases, as built-in SQL functions.

## Decision Drivers

1. **Time-Sortability**: IDs should be lexicographically or numerically ordered by creation time
2. **Uniqueness**: Zero collisions across distributed instances
3. **Compactness**: Efficient storage (64-bit integers better than 128-bit UUIDs when possible)
4. **URL Safety**: Some contexts require URL-safe characters only
5. **Standards Compliance**: Follow industry standards (RFC 9562 for UUID v7)
6. **Performance**: <1μs generation time, no network calls
7. **Distributed Support**: Work correctly across multiple server instances

## Considered Options

### Option 1: Single ID Function (❌ Rejected)
Provide only SNOWFLAKE_ID() or UUID_V7()

**Pros**:
- Simpler implementation
- Less documentation

**Cons**:
- Different use cases need different tradeoffs (integer vs string, storage size, URL safety)
- Forces one-size-fits-all solution

### Option 2: Three Complementary Functions (✅ Chosen)
Provide SNOWFLAKE_ID(), UUID_V7(), and ULID() for different use cases

**Pros**:
- Developers choose the right tool for their context
- Each function optimized for its purpose
- Clear migration paths between strategies

**Cons**:
- More functions to document and maintain

## Decision Outcome

**Chosen Option**: **Option 2 - Three Complementary Functions**

We implement three ID generation functions with distinct use cases:

### 1. SNOWFLAKE_ID() - Compact Integer IDs

**Use Case**: Primary keys, database sharding, numeric IDs  
**Return Type**: `BIGINT` (64-bit signed integer)  
**Format**: `[41-bit timestamp][10-bit node][12-bit sequence][1-bit sign]`

```sql
CREATE USER TABLE events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    event_type TEXT NOT NULL
);
```

**Structure**:
```
┌──────────────────┬──────────┬────────────┬─────┐
│   Timestamp      │  Node ID │  Sequence  │Sign │
│   (41 bits)      │ (10 bits)│  (12 bits) │(1)  │
└──────────────────┴──────────┴────────────┴─────┘
  Milliseconds since   0-1023      0-4095     0
  custom epoch         instances   per ms
```

**Characteristics**:
- **Size**: 8 bytes
- **Timestamp Resolution**: Milliseconds since epoch (2025-01-01T00:00:00Z)
- **Lifespan**: 41 bits = 69 years from epoch
- **Distributed Support**: Up to 1,024 nodes (10 bits)
- **Throughput**: 4,096 IDs per millisecond per node = 4M IDs/sec
- **Sortability**: Numeric ordering matches chronological ordering
- **Clock Skew Handling**: Falls back to sequence when clock moves backward

**Pros**:
- ✅ Most compact (8 bytes)
- ✅ Fastest integer comparisons
- ✅ Optimal for database indexing (B-tree performance)
- ✅ Compatible with existing integer-based systems
- ✅ Easy to extract timestamp component

**Cons**:
- ❌ Requires node_id configuration for distributed setups
- ❌ Limited to numeric contexts
- ❌ 69-year lifespan (acceptable for most applications)

**Implementation Notes**:
- Atomic sequence counter using `AtomicU16`
- Sequence resets to 0 each millisecond
- If sequence overflows (>4095), wait for next millisecond
- Clock skew detection: if timestamp < last_timestamp, keep last_timestamp + increment sequence

### 2. UUID_V7() - Standard RFC 9562 UUIDs

**Use Case**: Interoperability, microservices, API tokens, standards compliance  
**Return Type**: `TEXT` (36-character string)  
**Format**: `xxxxxxxx-xxxx-7xxx-xxxx-xxxxxxxxxxxx` (RFC 9562)

```sql
CREATE USER TABLE sessions (
    session_id TEXT PRIMARY KEY DEFAULT UUID_V7(),
    user_id BIGINT NOT NULL
);
```

**Structure**:
```
┌─────────────┬──────┬─────┬────────────────────┐
│  Timestamp  │ Ver  │Var  │      Random        │
│  (48 bits)  │(4bit)│(2b) │    (80 bits)       │
└─────────────┴──────┴─────┴────────────────────┘
  Unix ms       0111   10    Cryptographic random
```

**Characteristics**:
- **Size**: 16 bytes (stored as 36-char string with dashes)
- **Timestamp Resolution**: Milliseconds since Unix epoch (1970-01-01)
- **Lifespan**: 48 bits = 8,925 years from 1970
- **Uniqueness**: 80 bits random = 2^80 possible values per millisecond
- **Sortability**: Lexicographic string ordering matches chronological
- **Standards**: RFC 9562 compliant (successor to RFC 4122)

**Pros**:
- ✅ Industry standard (RFC 9562)
- ✅ Maximum interoperability (REST APIs, JSON, databases)
- ✅ Cryptographically random (no node_id needed)
- ✅ Extremely low collision probability (2^80 per ms)
- ✅ Long lifespan (8,925 years)

**Cons**:
- ❌ Larger storage (36 bytes as string, 16 bytes binary)
- ❌ Slower string comparisons vs integers
- ❌ Not URL-safe (contains dashes)

**Implementation Notes**:
- Uses Rust `uuid` crate with `v7` feature
- Generates via `Uuid::now_v7()`
- Formatted as standard hyphenated string

### 3. ULID() - URL-Safe Sortable IDs

**Use Case**: URLs, file names, logs, user-facing tokens, tracking IDs  
**Return Type**: `TEXT` (26-character string)  
**Format**: `01H4D6GCFRT7YGBHQM8QZJN5KE` (Crockford base32)

```sql
CREATE USER TABLE requests (
    request_id TEXT PRIMARY KEY DEFAULT ULID(),
    method TEXT NOT NULL,
    correlation_id TEXT DEFAULT ULID()  -- Non-PK usage
);
```

**Structure**:
```
┌──────────────┬──────────────────┐
│  Timestamp   │     Random       │
│  (48 bits)   │    (80 bits)     │
└──────────────┴──────────────────┘
  Unix ms         Crockford base32
  (10 chars)      (16 chars)
```

**Characteristics**:
- **Size**: 16 bytes (stored as 26-char string)
- **Timestamp Resolution**: Milliseconds since Unix epoch
- **Lifespan**: 48 bits = 8,925 years from 1970
- **Uniqueness**: 80 bits random = 2^80 possible values per millisecond
- **Sortability**: Lexicographic string ordering matches chronological
- **Character Set**: Crockford base32 (0-9, A-Z excluding I, L, O, U)

**Pros**:
- ✅ URL-safe (no dashes, no special characters)
- ✅ Case-insensitive (Crockford base32)
- ✅ Human-readable (easier to type than UUID)
- ✅ Compact string representation (26 vs 36 chars for UUID)
- ✅ No node_id needed (random component)
- ✅ Copy-paste friendly (no hyphens to skip)

**Cons**:
- ❌ Not an official standard (community convention)
- ❌ Less ecosystem support than UUID
- ❌ Larger than SNOWFLAKE_ID for integer contexts

**Implementation Notes**:
- Uses Rust `ulid` crate
- Generates via `Ulid::new()`
- String representation via `to_string()` (Crockford base32)

## Comparison Table

| Criterion | SNOWFLAKE_ID | UUID_V7 | ULID |
|-----------|--------------|---------|------|
| **Storage Size** | 8 bytes | 16 bytes (36 as string) | 16 bytes (26 as string) |
| **Type** | BIGINT | TEXT | TEXT |
| **Sortability** | Numeric | Lexicographic | Lexicographic |
| **URL Safe** | ✅ Yes | ❌ No (dashes) | ✅ Yes |
| **Standards** | ❌ Twitter/Snowflake | ✅ RFC 9562 | ⚠️ Community |
| **Distributed** | Requires node_id | Cryptographic random | Cryptographic random |
| **Throughput** | 4M/sec/node | Unlimited | Unlimited |
| **Lifespan** | 69 years (2094) | 8,925 years (10,895) | 8,925 years (10,895) |
| **Collision Risk** | Zero (sequence) | 2^-80 per ms | 2^-80 per ms |
| **Index Performance** | ⭐⭐⭐ Best | ⭐⭐ Good | ⭐⭐ Good |
| **Human Friendly** | ❌ Numbers only | ⚠️ Has dashes | ✅ Clean string |

## Decision Rationale

We provide all three functions because:

1. **SNOWFLAKE_ID**: Best for performance-critical primary keys in high-throughput systems
2. **UUID_V7**: Best for interoperability with external systems and standard compliance
3. **ULID**: Best for user-facing identifiers (URLs, logs, correlation tracking)

### Use Case Examples

**SNOWFLAKE_ID**:
```sql
-- High-throughput event logging
CREATE USER TABLE events (
    id BIGINT PRIMARY KEY DEFAULT SNOWFLAKE_ID(),
    user_id BIGINT NOT NULL,
    event_type TEXT
);
```

**UUID_V7**:
```sql
-- REST API resources exposed to external systems
CREATE SHARED TABLE tenants (
    tenant_id TEXT PRIMARY KEY DEFAULT UUID_V7(),
    name TEXT NOT NULL
);
```

**ULID**:
```sql
-- Request tracking across distributed services
CREATE STREAM TABLE http_logs (
    request_id TEXT PRIMARY KEY DEFAULT ULID(),
    correlation_id TEXT DEFAULT ULID(),
    path TEXT NOT NULL
);
```

## Validation

### Monotonicity Test
```sql
-- Insert 1000 rows, verify IDs increase
INSERT INTO test (SELECT FROM generate_series(1, 1000));
SELECT COUNT(*) FROM test WHERE id > LAG(id) OVER (ORDER BY created_at);
-- Expected: 999 (all transitions are increasing)
```

### Uniqueness Test
```sql
-- 10K concurrent inserts
SELECT COUNT(DISTINCT id) as unique_ids, COUNT(*) as total 
FROM test;
-- Expected: unique_ids == total
```

### Timestamp Extraction Test
```sql
-- SNOWFLAKE_ID: Extract timestamp component
SELECT (id >> 23) as timestamp_ms FROM events;

-- UUID_V7: First 12 hex chars = timestamp
SELECT SUBSTRING(event_id, 1, 8) as timestamp_hex FROM events;
```

## Follow-up

- Document node_id configuration in deployment guide
- Add monitoring for sequence overflow warnings
- Consider CUID2 or KSUID if additional formats requested
- Add timestamp extraction helper functions (SNOWFLAKE_TIMESTAMP, ULID_TIMESTAMP)

## Links

- Implementation: `/backend/crates/kalamdb-core/src/sql/functions/`
- [RFC 9562 - UUID Version 7](https://www.rfc-editor.org/rfc/rfc9562.html)
- [ULID Specification](https://github.com/ulid/spec)
- [Twitter Snowflake](https://github.com/twitter-archive/snowflake/tree/snowflake-2010)
- [Crockford Base32](https://www.crockford.com/base32.html)
