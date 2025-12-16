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
import { createClient, TimestampFormatter, Auth } from '@kalamdb/client';

// Configure formatter when creating client
const client = createClient({
  url: 'http://localhost:8080',
  auth: Auth.basic('admin', 'admin'),
  timestampFormat: 'iso8601' // Default
});

// Or create standalone formatter
const formatter = new TimestampFormatter({ format: 'iso8601' });

// Format a timestamp
const timestamp = 1734191445123;
console.log(formatter.format(timestamp)); 
// Output: "2024-12-14T15:30:45.123Z"

// Format relative time
console.log(formatter.formatRelative(Date.now() - 7200000));
// Output: "2 hours ago"

// Format query results
const results = await client.query('SELECT * FROM system.users');
results.rows.forEach(row => {
  console.log(`User created: ${formatter.format(row.created_at)}`);
});

// Change format dynamically
formatter.setFormat('relative');
console.log(formatter.format(timestamp));
// Output: "5 minutes ago" (relative to current time)
```

### Different Formats

```typescript
// ISO 8601 with milliseconds (default)
const iso = new TimestampFormatter({ format: 'iso8601' });
console.log(iso.format(1734191445123));
// "2024-12-14T15:30:45.123Z"

// Date only
const date = new TimestampFormatter({ format: 'iso8601-date' });
console.log(date.format(1734191445123));
// "2024-12-14"

// Unix seconds
const unix = new TimestampFormatter({ format: 'unix-sec' });
console.log(unix.format(1734191445123));
// "1734191445"

// Relative time
const relative = new TimestampFormatter({ format: 'relative' });
console.log(relative.format(Date.now() - 3600000));
// "1 hour ago"

// RFC 2822 (email format)
const rfc = new TimestampFormatter({ format: 'rfc2822' });
console.log(rfc.format(1734191445123));
// "Fri, 14 Dec 2024 15:30:45 +0000"
```

### Parsing Timestamps

```typescript
import { parseISO8601, now } from '@kalamdb/client';

// Parse ISO 8601 back to milliseconds
const ms = parseISO8601("2024-12-14T15:30:45.123Z");
console.log(ms); // 1734191445123

// Get current timestamp (compatible with KalamDB)
const currentMs = now();
console.log(currentMs); // 1734191445123
```

### System Tables

All system tables return timestamps as milliseconds:

```typescript
// Query system.users
const users = await client.query('SELECT * FROM system.users');

// Format timestamps in results
const formatter = new TimestampFormatter({ format: 'iso8601' });
users.rows.forEach(user => {
  console.log(`User: ${user.username}`);
  console.log(`Created: ${formatter.format(user.created_at)}`);
  console.log(`Updated: ${formatter.format(user.updated_at)}`);
  if (user.last_login_at) {
    console.log(`Last login: ${formatter.formatRelative(user.last_login_at)}`);
  }
});
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

### Client (TypeScript)

- **Rust Implementation**: Formatting logic in `kalam-link` Rust crate
- **WASM Bindings**: Exposed via `WasmTimestampFormatter`
- **TypeScript Wrapper**: Type-safe API wrapping WASM functions
- **Benefits**:
  - Single source of truth (Rust code)
  - Consistent across all SDKs (TypeScript, Python, future languages)
  - High performance (compiled Rust via WASM)

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

## Future Enhancements

Planned for future releases:

1. **Custom Format Strings**:
   ```typescript
   formatter.setFormat('%Y-%m-%d %H:%M:%S');
   ```

2. **Timezone Support**:
   ```typescript
   formatter.setTimezone('America/New_York');
   ```

3. **Locale-Aware Formatting**:
   ```typescript
   formatter.setLocale('fr-FR');
   ```

4. **Python SDK**:
   Same Rust implementation via Python bindings

## Migration Notes

### Before (TypeScript-only formatting)

```typescript
const date = new Date(timestamp);
console.log(date.toISOString());
```

### After (Rust-based formatting)

```typescript
const formatter = new TimestampFormatter({ format: 'iso8601' });
console.log(formatter.format(timestamp));
```

**Benefits**: Consistent formatting across all SDKs, single source of truth.

## Related

- [SQL Data Types](./SQL.md#data-types)
- [System Tables](./API.md#system-tables)
- [TypeScript SDK](./SDK.md#typescript)
