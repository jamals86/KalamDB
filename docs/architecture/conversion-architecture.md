# ScalarValue → JSON Conversion Architecture

## Single Source of Truth ✅

All `ScalarValue` → `JSON` conversions in KalamDB flow through **ONE central function**: `scalar_value_to_json_with_mode()` in `arrow_json_conversion.rs`.

## Conversion Hierarchy

```
scalar_value_to_json_with_mode()  ← SINGLE SOURCE OF TRUTH
         ↓                           (converts individual ScalarValue to JsonValue)
    ┌────┴────┐
    ↓         ↓
row_to_json_map()              record_batch_to_json_rows()
    ↓                                   ↓
    Converts Row                        Converts RecordBatch
    (Map of ScalarValues)              (Columnar Arrow format)
    ↓                                   ↓
    Used by:                            Used by:
    • WebSocket notifications           • REST API /v1/api/sql
    • WebSocket subscriptions
    • WebSocket batch data
```

## Implementation Files

### Core Conversion Module
**File:** `backend/crates/kalamdb-core/src/providers/arrow_json_conversion.rs`

**Functions:**
1. **`scalar_value_to_json_with_mode(value, mode)`** - SINGLE SOURCE OF TRUTH
   - Converts individual ScalarValue to JSON
   - Supports two modes: Simple (plain JSON) and Typed (with type wrappers)
   - All other conversions MUST call this function

2. **`row_to_json_map(row, mode)`** - Row converter
   - Converts Row (HashMap<String, ScalarValue>) to HashMap<String, JsonValue>
   - Internally calls `scalar_value_to_json_with_mode()` for each column
   - Used by WebSocket handlers

3. **`record_batch_to_json_rows(batch, mode)`** - RecordBatch converter
   - Converts Arrow RecordBatch to Vec<HashMap<String, JsonValue>>
   - Internally calls `scalar_value_to_json_with_mode()` for each cell
   - Used by REST API

### Usage Points (All Use Centralized Functions ✅)

#### 1. REST API - SQL Query Results
**File:** `backend/crates/kalamdb-api/src/handlers/sql_handler.rs`
```rust
// Converts query results to JSON
let rows = record_batch_to_json_rows(&batch, mode)?;
```

#### 2. WebSocket - Initial Subscription Data
**File:** `backend/crates/kalamdb-api/src/handlers/events/subscription.rs`
```rust
// Converts initial rows to JSON
let json_row = row_to_json_map(&row, subscription.options.serialization_mode)?;
```

#### 3. WebSocket - Batch Data
**File:** `backend/crates/kalamdb-api/src/handlers/events/batch.rs`
```rust
// Converts batch rows to JSON
let json_row = row_to_json_map(&row, serialization_mode)?;
```

#### 4. WebSocket - Change Notifications
**File:** `backend/crates/kalamdb-core/src/live/notification.rs`
```rust
// Converts changed rows to JSON for notifications
let new_row_json = row_to_json_map(&new_row, subscription.serialization_mode)?;
let old_row_json = row_to_json_map(&old_row, subscription.serialization_mode)?;
```

## Serialization Modes

### Simple Mode (Default for REST API)
```json
{
  "id": "123",           // Int64 always as string
  "name": "Alice",       // Utf8 as plain string
  "age": 30,             // Int32 as number
  "active": true         // Boolean as boolean
}
```

### Typed Mode (Default for WebSocket/SDK)
```json
{
  "id": {"Int64": "123"},        // With type wrapper
  "name": {"Utf8": "Alice"},     // With type wrapper
  "age": {"Int32": 30},          // With type wrapper
  "active": {"Boolean": true}    // With type wrapper
}
```

## Guarantees

✅ **NO duplicate conversion logic** - All conversions use the same code path

✅ **NO direct ScalarValue serialization** - All conversions go through centralized functions

✅ **Consistent precision handling** - Int64/UInt64 always serialized as strings in both modes

✅ **Consistent type information** - Typed mode uses StoredScalarValue format everywhere

✅ **Single point of maintenance** - Changes to conversion logic only need to happen in one place

## Verification

To verify no rogue conversions exist, search for:
```bash
# Should find NO matches outside arrow_json_conversion.rs
grep -r "serde_json::to_value.*ScalarValue" backend/crates/kalamdb-api/
grep -r "serde_json::to_value.*ScalarValue" backend/crates/kalamdb-core/src/live/
```

## Testing

All conversion modes are tested in `arrow_json_conversion.rs`:
- `test_simple_mode_int64_always_string()` - Verifies Int64 → string conversion
- `test_typed_mode_int64_with_wrapper()` - Verifies typed format
- `test_serialization_mode_default()` - Verifies default behavior
- And 10+ more tests covering all data types

## Future Maintenance

**When adding new data types:**
1. Update `scalar_value_to_json_simple()` for Simple mode
2. Update `StoredScalarValue` in `kalamdb_commons::models::row` for Typed mode
3. Both high-level functions (`row_to_json_map` and `record_batch_to_json_rows`) will automatically support the new type

**When changing conversion logic:**
- Only modify `scalar_value_to_json_with_mode()` or `scalar_value_to_json_simple()`
- All usage points (REST API, WebSocket, notifications) will automatically get the update
