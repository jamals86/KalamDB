# ADR-014: Type-Safe Wrappers for Identifiers

**Status**: Accepted  
**Date**: 2025-10-25  
**Author**: KalamDB Team  
**Context**: User Story 6 (Code Quality) - T415

## Context and Problem Statement

KalamDB uses several string-based identifiers throughout the codebase:
- `user_id` - Identifies users
- `namespace_id` - Identifies namespaces (databases)
- `table_name` - Identifies tables within a namespace
- `storage_id` - Identifies storage backends

These identifiers were initially represented as plain `String` types, leading to several issues:

1. **Type Confusion**: A function expecting a `user_id` could accidentally receive a `table_name` without compile-time detection
2. **API Clarity**: Function signatures like `fn get_data(id1: String, id2: String, id3: String)` provide no semantic meaning
3. **Refactoring Safety**: Changing identifier formats requires manual inspection of all String usages
4. **Domain Modeling**: Business concepts are not clearly represented in the type system

## Decision Drivers

- **Type Safety**: Prevent identifier type confusion at compile time
- **Code Clarity**: Make APIs self-documenting through meaningful types
- **Maintainability**: Enable confident refactoring with compiler assistance
- **Zero Cost**: No runtime overhead compared to plain strings
- **Ergonomics**: Easy conversion to/from strings for serialization and display

## Considered Options

### Option 1: Continue Using Plain Strings
**Pros**: Simple, no changes needed  
**Cons**: No type safety, error-prone, unclear APIs

### Option 2: Type Aliases (`type UserId = String`)
**Pros**: Minimal code changes, documentation benefit  
**Cons**: No actual type safety (aliases are transparent to compiler)

### Option 3: Newtype Pattern with Wrapper Structs (SELECTED)
**Pros**: Full type safety, zero runtime cost, clear semantics  
**Cons**: Requires conversions, more initial code

### Option 4: Enums for All Identifiers
**Pros**: Pattern matching capabilities  
**Cons**: Overkill for simple identifiers, memory overhead

## Decision Outcome

**Chosen Option**: **Newtype Pattern with Wrapper Structs**

We implement type-safe wrappers for all identifier types using the newtype pattern:

```rust
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct UserId(String);

impl UserId {
    pub fn new(id: impl Into<String>) -> Self {
        Self(id.into())
    }
    
    pub fn as_str(&self) -> &str {
        &self.0
    }
    
    pub fn into_string(self) -> String {
        self.0
    }
}

impl From<String> for UserId {
    fn from(s: String) -> Self {
        Self(s)
    }
}

impl From<&str> for UserId {
    fn from(s: &str) -> Self {
        Self(s.to_string())
    }
}

impl AsRef<str> for UserId {
    fn as_ref(&self) -> &str {
        &self.0
    }
}

impl Display for UserId {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
```

### Type-Safe Wrappers Implemented

1. **`UserId`** (`kalamdb-commons/src/models/user_id.rs`)
   - Wraps user identifiers
   - Used in: Authentication, table ownership, live queries

2. **`NamespaceId`** (`kalamdb-commons/src/models/namespace_id.rs`)
   - Wraps namespace (database) identifiers
   - Used in: Table creation, queries, storage management

3. **`TableName`** (`kalamdb-commons/src/models/table_name.rs`)
   - Wraps table identifiers
   - Used in: DDL commands, queries, subscriptions

4. **`StorageId`** (`kalamdb-commons/src/models/storage_id.rs`)
   - Wraps storage backend identifiers
   - Used in: Storage configuration, table assignment

## Implementation Strategy

### Phase 1: Create Wrapper Types ✅ COMPLETE
- Define newtype structs in `kalamdb-commons`
- Implement conversion traits (`From`, `Into`, `AsRef`, `Display`)
- Add serde support via feature flag
- Export from `kalamdb-commons/lib.rs`

### Phase 2: Migrate Core Models ✅ COMPLETE
- `TableDefinition` struct fields
- DDL statement structs (`SubscribeStatement`, `FlushTableStatement`, etc.)
- System table provider records (`LiveQueryRecord`)

### Phase 3: Update Business Logic ✅ PARTIAL
- SQL executor comparisons (use `.as_ref()`)
- Service layer method signatures
- API handlers

### Phase 4: Update Tests ✅ PARTIAL
- Replace String literals with wrapper constructors
- Update assertions to use `.as_ref()`

## Usage Patterns

### Creating Instances

```rust
// From string literal
let user_id = UserId::from("user123");
let table_name = TableName::new("messages");

// From owned String
let user_id = UserId::new(some_string);

// In struct initialization
let sub = SubscribeStatement {
    namespace: NamespaceId::from("prod"),
    table_name: TableName::from("users"),
};
```

### Converting to String

```rust
// Borrowing as &str
let user_str: &str = user_id.as_ref();
println!("User: {}", user_id.as_str());

// Consuming into String
let user_string: String = user_id.into_string();

// Automatic via Display trait
format!("User: {}", user_id);
```

### Comparing with Strings

```rust
// Use .as_ref() for comparisons
if stmt.namespace.as_ref() == "default" {
    // ...
}

// Or pattern matching
match table_name.as_ref() {
    "users" => { /* ... */ }
    "messages" => { /* ... */ }
    _ => { /* ... */ }
}
```

### Serialization (with serde feature)

```rust
#[derive(Serialize, Deserialize)]
struct ApiRequest {
    user_id: UserId,
    table_name: TableName,
}

// Serializes as: {"user_id": "user123", "table_name": "messages"}
```

## Benefits Realized

### Type Safety
```rust
// Compile error - prevents accidental misuse
fn get_user_data(user_id: UserId) -> Result<Data> { /* ... */ }

let table = TableName::from("messages");
get_user_data(table);  // ❌ Compile error!
```

### Self-Documenting APIs
```rust
// Before
fn create_table(id1: String, id2: String, id3: String) -> Result<()>

// After
fn create_table(
    namespace: NamespaceId,
    table_name: TableName,
    storage_id: StorageId
) -> Result<()>
```

### Refactoring Confidence
- Changing `UserId` format? Compiler finds all usages
- Adding validation? Single implementation point
- Migrating storage? Type system guides changes

## Consequences

### Positive
- **Compile-Time Safety**: Identifier type mismatches caught by compiler
- **Code Clarity**: Function signatures are self-documenting
- **Zero Runtime Cost**: Newtype pattern has no overhead (wrapper optimized away)
- **Consistent Conversions**: Single implementation of From/Into traits
- **IDE Support**: Better autocomplete and type inference

### Negative
- **Verbosity**: Requires explicit conversions (`user_id.as_ref()`, `TableName::from()`)
- **Migration Cost**: Existing String-based code requires updates
- **Learning Curve**: New developers must understand wrapper pattern

### Neutral
- **Test Updates**: Tests need wrapper constructors instead of string literals
- **Serialization**: Serde feature flag required for JSON APIs

## Implementation Status

### Completed (Tasks T359-T363, T376-T382)
- ✅ Created all four wrapper types
- ✅ Migrated `TableDefinition` struct
- ✅ Migrated DDL statements (`SubscribeStatement`, `FlushTableStatement`, `FlushAllTablesStatement`)
- ✅ Migrated `LiveQueryRecord` in system table providers
- ✅ Updated SQL executor comparison sites
- ✅ Updated live query manager

### Remaining Work
- [ ] Migrate `UserRecord` in users_provider.rs
- [ ] Migrate kalamdb-sql models (`Table`, `LiveQuery`, `NamespaceInfo`, etc.)
- [ ] Migrate storage command statements
- [ ] Complete test suite updates

## Related Decisions

- **ADR-013**: Model Organization (separating wrapper types into dedicated files)
- **ADR-015**: Enum Usage Policy (when to use enums vs wrappers)

## References

- Rust Newtype Pattern: https://doc.rust-lang.org/rust-by-example/generics/new_types.html
- Type-Driven Design: https://fsharpforfunandprofit.com/series/designing-with-types.html
- Implementation: `backend/crates/kalamdb-commons/src/models/`

## Notes

The newtype pattern is a zero-cost abstraction in Rust. The wrapper struct is compile-time only and has no runtime representation - it's optimized away to the underlying `String` type in release builds.

All wrappers implement `AsRef<str>` which enables transparent usage in many string contexts without explicit conversion.
