# Timestamp Formatting in KalamDB

## Overview

KalamDB stores all timestamps as **milliseconds since Unix epoch** (i64) for efficiency. This provides:
- **Compact wire format**: 8 bytes (vs 24+ bytes for ISO strings)
- **Fast comparisons**: Integer arithmetic
- **Language-agnostic**: Works across all platforms

Client libraries provide **configurable formatting** to display timestamps in human-readable formats.

## Architecture

```
┌─────────────┐
│   Backend   │  Stores: i64 (milliseconds)
│  (Rust)     │  Arrow: Timestamp(Millisecond, None)
└──────┬──────┘  JSON: sends integer
       │
       ▼
┌─────────────┐
│  Wire       │  { "created_at": 1734191445123 }
│  (JSON)     │  Efficient 8-byte transmission
└──────┬──────┘
       │
       ▼
┌─────────────┐
│  Client     │  Rust formatter (WASM for TypeScript)
│  (SDK)      │  Configurable display format
└─────────────┘  "2024-12-14T15:30:45.123Z"
```

## Supported Formats

| Format | Example Output | Use Case |
|--------|---------------|----------|
| `iso8601` | `2024-12-14T15:30:45.123Z` | Default, universal format |
| `iso8601-date` | `2024-12-14` | Date-only display |
| `iso8601-datetime` | `2024-12-14T15:30:45Z` | No milliseconds |
| `unix-ms` | `1734191445123` | Raw milliseconds |
| `unix-sec` | `1734191445` | Unix seconds |
| `relative` | `2 hours ago` | Human-friendly relative time |
| `rfc2822` | `Fri, 14 Dec 2024 15:30:45 +0000` | Email headers |
| `rfc3339` | `2024-12-14T15:30:45.123+00:00` | RFC 3339 standard |

## Usage Examples

### TypeScript/JavaScript

```typescript
const ms = 1734191445123;
console.log(new Date(ms).toISOString());
// "2024-12-14T15:30:45.123Z"
```

### Rust

```rust
use chrono::{DateTime, Utc};

let ms: i64 = 1734191445123;
let dt = DateTime::<Utc>::from_timestamp_millis(ms).unwrap();
println!("{}", dt.to_rfc3339());
```

## Implementation Details

### Backend (Rust)

- **Storage**: i64 (8 bytes) - milliseconds since epoch
- **Arrow Schema**: `DataType::Timestamp(TimeUnit::Millisecond, None)`
  - No timezone metadata → sends integers over JSON (efficient)
- **System Tables**: All `created_at`, `updated_at`, `deleted_at` fields use this format

### Wire Format (JSON)

```json
{
  "username": "admin",
  "created_at": 1734191445123,
  "updated_at": 1734191445123,
  "last_login_at": null
}
```

Timestamps are **always sent as numbers** (8 bytes) not strings (24+ bytes).

### Client (Any Language)

- Treat the value as milliseconds since Unix epoch
- Convert/format at the UI boundary (logs, dashboards, user-facing displays)

## Performance Considerations

### Wire Format Efficiency

```
Integer format:  { "created_at": 1734191445123 }        (8 bytes)
String format:   { "created_at": "2024-12-14T..." }    (24 bytes)

Savings: 66% smaller wire format
```

### Query Results

For a query returning 10,000 rows with 3 timestamp columns:
- **Integer format**: ~240 KB
- **String format**: ~720 KB
- **Bandwidth saved**: 480 KB (66%)

### Client-Side Formatting

Formatting happens **only when displaying** to users:
- Query results keep raw integers (efficient)
- Format only visible rows (UI pagination)
- Zero overhead for machine-to-machine communication

## Related

- [SQL Data Types](./sql.md#data-types)
- [REST API Reference](../api/api-reference.md)
- [TypeScript/JavaScript SDK](../sdk/sdk.md)
