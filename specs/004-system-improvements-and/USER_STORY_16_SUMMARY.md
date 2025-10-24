# User Story 16: Data Type Standardization - Summary

**Date**: 2025-10-24  
**Status**: ✅ Added to spec.md  
**Priority**: P1 (High - Data Correctness Issue)

## What Was Added

Added **User Story 16** to `/specs/004-system-improvements-and/spec.md` addressing the critical gap between SQL parser type support and flush operation type support.

## The Problem (Root Cause)

**Current State**:
- ✅ SQL Parser (`kalamdb-sql/src/compatibility.rs`): Supports 30+ data types
- ❌ Flush Operation (`kalamdb-core/src/flush/user_table_flush.rs`): Only supports 3 types (Utf8, Int64, Boolean)

**Impact**:
- Users can create tables with TIMESTAMP, FLOAT, DATE, etc.
- Inserts succeed and data goes into RocksDB buffer
- **Flush FAILS** with "Unsupported data type for flush: Timestamp(Millisecond, None)"
- Data stuck in buffer, jobs marked as failed
- No upfront validation - users discover issue after inserting data

## The Solution

### Simplified Type System (10 Basic Types)

Based on your preferences, we defined exactly **10 basic data types**:

| KalamDB Type | DataFusion Type | SQL Aliases | Storage |
|--------------|-----------------|-------------|---------|
| **BOOLEAN** | `DataType::Boolean` | BOOL | 2 bytes (tag + value) |
| **INT** | `DataType::Int32` | INTEGER, INT4 | 5 bytes (tag + i32) |
| **BIGINT** | `DataType::Int64` | INT8 | 9 bytes (tag + i64) |
| **DOUBLE** | `DataType::Float64` | FLOAT8, DOUBLE PRECISION | 9 bytes (tag + f64) |
| **TEXT** | `DataType::Utf8` | VARCHAR, STRING | Variable (tag + len + UTF-8) |
| **TIMESTAMP** | `DataType::Timestamp(TimeUnit::Microsecond, None)` | DATETIME | 9 bytes (tag + i64 microseconds) |
| **DATE** | `DataType::Date32` | - | 5 bytes (tag + i32 days) |
| **TIME** | `DataType::Time64(TimeUnit::Microsecond)` | - | 9 bytes (tag + i64 microseconds) |
| **JSON** | `DataType::Utf8` (serialized) | OBJECT | Variable (tag + len + JSON text) |
| **BYTES** | `DataType::Binary` | BINARY, BYTEA, BLOB | Variable (tag + len + bytes) |

**Future Addition**: **OBJECT** type with JSON Schema validation (queryable structured data)

### Centralized Architecture (One Place for Everything)

Created new directory structure in `kalamdb-commons/src/types/`:

```
backend/crates/kalamdb-commons/src/types/
├── mod.rs           - Public API exports
├── core.rs          - KalamDbType enum (THE single source of truth)
├── conversions.rs   - Arrow ↔ KalamDB type conversions
├── codec.rs         - RocksDB encoding/decoding (JSON ↔ Bytes)
├── parquet.rs       - Parquet serialization (JSON ↔ Arrow arrays)
└── validation.rs    - Type validation and schema checking
```

**Key Design Principle**: 
> **ALL data type handling MUST go through this module**. No direct Arrow type manipulation elsewhere in codebase.

### Adding New Type (6-Step Process)

To add a new type (e.g., DECIMAL in future), update exactly 6 places:

1. **`core.rs`**: Add enum variant `KalamDbType::Decimal`
2. **`conversions.rs::to_arrow()`**: Add `KalamDbType::Decimal => DataType::Decimal(38, 10)`
3. **`conversions.rs::from_arrow()`**: Add `DataType::Decimal(_, _) => Ok(KalamDbType::Decimal)`
4. **`codec.rs::encode_value()`**: Add encoding logic with new type tag (e.g., `0x0B`)
5. **`codec.rs::decode_value()`**: Add decoding logic for tag `0x0B`
6. **`parquet.rs::json_to_arrow_array()`**: Add JSON → Arrow array conversion

Compiler enforces all 6 changes via exhaustive match checking.

### Type-Tagged RocksDB Storage Format

Each value stored in RocksDB has a **type tag byte** for safety:

```
Format: [Type Tag 1 byte][Data N bytes]

Examples:
- Boolean true:  [0x01][0x01]
- INT 42:        [0x02][2A 00 00 00]  (little-endian i32)
- TEXT "hello":  [0x05][05 00 00 00][68 65 6C 6C 6F]  (length + UTF-8)
- TIMESTAMP:     [0x06][microseconds as i64]
- NULL:          [0xFF]
```

**Benefits**:
1. **Type-safe reads**: Decode function returns (KalamDbType, JsonValue) tuple
2. **Future-proof**: Can change encoding per type without breaking others
3. **Versioned**: Reserved 0xF0-0xFF for version tags
4. **Debuggable**: Can inspect raw bytes and identify type

### Architectural Requirements (10 ARs)

Added strict architectural requirements to ensure:

- **AR-001**: All type conversions go through `kalamdb-commons/src/types/` (no shortcuts)
- **AR-002**: RocksDB storage MUST be type-tagged (first byte = type ID)
- **AR-003**: Type encoding MUST be versioned (0xF0-0xFF reserved for versions)
- **AR-004**: Encoding MUST be deterministic (same value → same bytes)
- **AR-005**: Microsecond precision for all temporal types (modern database standard)
- **AR-006**: JSON type MUST validate syntax on INSERT (fail-fast)
- **AR-007**: BYTES type MUST accept hex (0x...) and base64 formats
- **AR-008**: Adding new type MUST touch exactly 6 locations (compiler-enforced)
- **AR-009**: Type errors MUST include helpful messages (expected vs actual)
- **AR-010**: Future OBJECT type MUST support JSON Schema validation

## Integration Tests (12 Tests)

New test file: `backend/tests/integration/test_data_type_flush.rs`

1. ✅ test_flush_timestamp_microsecond_precision
2. ✅ test_flush_double_values
3. ✅ test_flush_date_values
4. ✅ test_flush_time_values
5. ✅ test_flush_json_validated_on_insert
6. ✅ test_flush_bytes_hex_and_base64
7. ✅ test_flush_int_and_bigint
8. ✅ test_flush_all_10_types_combined (10 columns, 100 rows)
9. ✅ test_flush_null_values_all_types
10. ✅ test_create_table_with_unsupported_type_fails (fail-fast validation)
11. ✅ test_shared_table_flush_all_types
12. ✅ test_roundtrip_rocksdb_parquet_query (insert → buffer → flush → query)

## Migration Timeline

**Immediate (Week 1)**: 
- Create `kalamdb-commons/src/types/` directory
- Implement 10 basic types in `core.rs`, `conversions.rs`, `codec.rs`, `parquet.rs`
- Add 100+ unit tests for type conversions

**Week 2**: 
- Migrate CREATE TABLE validation to use `KalamDbType::from_arrow()`
- Add fail-fast error messages for unsupported types

**Week 3**: 
- Update flush operations (`user_table_flush.rs`, `shared_table_flush.rs`)
- Replace hardcoded type matching with centralized `JsonToArrowConverter`

**Week 4**: 
- Create integration tests (12 tests in `test_data_type_flush.rs`)
- Run full regression suite

**Month 2**: 
- Add OBJECT type with JSON Schema support
- Enable queryable JSON fields

## Not Supported (v1.0)

Explicitly excluded types (clear documentation for users):

❌ **FLOAT** (32-bit) - Use DOUBLE instead for consistency  
❌ **SMALLINT/TINYINT** - Use INT for simplicity  
❌ **UNSIGNED types** - Use BIGINT for larger positive ranges  
❌ **DECIMAL/NUMERIC** - Deferred to v1.1 (requires rust_decimal library)  
❌ **UUID (native)** - Use TEXT with UUID_V7() function  
❌ **INTERVAL** - Complex temporal arithmetic, deferred  
❌ **Array types** - Deferred to OBJECT implementation  
❌ **Struct types** - Deferred to OBJECT implementation  

## Future Roadmap

- **v1.1**: OBJECT type with JSON Schema validation
- **v1.2**: DECIMAL type with arbitrary precision (rust_decimal crate)
- **v2.0**: Array types with element validation
- **v2.1**: Nested Struct types

## Benefits

1. ✅ **Fixes flush failures**: TIMESTAMP, DOUBLE, DATE, TIME, JSON, BYTES all supported
2. ✅ **Fail-fast validation**: Errors at CREATE TABLE, not at flush time
3. ✅ **Single source of truth**: One place for all type logic
4. ✅ **Easy to extend**: Clear 6-step process for new types
5. ✅ **Type-safe storage**: Tagged encoding prevents corruption
6. ✅ **Future-proof**: Versioned encoding supports migration
7. ✅ **Testable**: Isolated modules with comprehensive tests
8. ✅ **Performance tunable**: Can optimize per-type encoding
9. ✅ **Clear documentation**: Users know exactly what's supported
10. ✅ **Flexible for future**: OBJECT type ready for JSON Schema

---

**Status**: Ready for implementation  
**Next Step**: Create `kalamdb-commons/src/types/` directory and implement core.rs
