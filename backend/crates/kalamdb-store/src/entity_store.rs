//! Type-safe entity storage with generic key types.
//!
//! This module provides the new `EntityStore<K, V>` trait which uses type-safe keys
//! (instead of strings) to provide compile-time safety and prevent wrong-key bugs.
//!
//! ## Architecture
//!
//! ```text
//! EntityStore<K, V>        ← Typed entity CRUD with generic keys (this file)
//!     ↓
//! StorageBackend           ← Generic K/V operations (storage_trait.rs)
//!     ↓
//! RocksDB/Sled/etc         ← Actual storage implementation
//! ```
//!
//! ## Key Differences from Old EntityStore<T>
//!
//! - **Old**: `EntityStore<T>` used string keys, single type parameter
//! - **New**: `EntityStore<K, V>` uses type-safe keys, two type parameters
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_store::{EntityStore, StorageBackend};
//! use kalamdb_commons::{StorageKey, UserId, User};
//! use std::sync::Arc;
//!
//! struct UserStore {
//!     backend: Arc<dyn StorageBackend>,
//! }
//!
//! impl EntityStore<UserId, User> for UserStore {
//!     fn backend(&self) -> &Arc<dyn StorageBackend> {
//!         &self.backend
//!     }
//!     
//!     fn partition(&self) -> &str {
//!         "system_users"
//!     }
//! }
//!
//! // Type-safe usage:
//! let user_id = UserId::new("u1");
//! let user = User { id: user_id.clone(), name: "Alice".into(), ... };
//! store.put(&user_id, &user).unwrap();
//! let retrieved = store.get(&user_id).unwrap().unwrap();
//! ```

use crate::storage_trait::{Partition, Result, StorageBackend, StorageError};
use kalamdb_commons::StorageKey;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Trait for typed entity storage with type-safe keys and automatic serialization.
///
/// This trait provides strongly-typed CRUD operations with compile-time key safety.
/// Unlike the old `EntityStore<T>` which used string keys, this version uses
/// generic key types (K) that implement `StorageKey`.
///
/// ## Type Parameters
/// - `K`: Key type that implements StorageKey (UserId, RowId, TableId, etc.)
/// - `V`: Value/entity type that must be Serialize + Deserialize
///
/// ## Required Methods
/// - `backend()`: Returns reference to the storage backend
/// - `partition()`: Returns partition name for this entity type
///
/// ## Provided Methods (with default JSON serialization)
/// - `serialize()`: Entity → bytes (default: JSON, override for bincode)
/// - `deserialize()`: bytes → Entity (default: JSON, override for bincode)
/// - `put()`: Store an entity by key
/// - `get()`: Retrieve an entity by key
/// - `delete()`: Remove an entity by key
/// - `scan_prefix()`: Scan entities with key prefix
/// - `scan_all()`: Scan all entities in partition
///
/// ## Type Safety Benefits
///
/// ```rust,ignore
/// // Compile-time safety prevents wrong keys:
/// let user_id = UserId::new("u1");
/// let table_id = TableId::new(...);
///
/// user_store.get(&user_id)   // ✓ Compiles
/// user_store.get(&table_id)  // ✗ Compile error - wrong key type!
/// ```
pub trait EntityStore<K, V>
where
    K: StorageKey,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    /// Returns a reference to the storage backend.
    fn backend(&self) -> &Arc<dyn StorageBackend>;

    /// Returns the partition name for this entity type.
    ///
    /// Examples: "system_users", "system_jobs", "user_table:default:users"
    fn partition(&self) -> &str;

    /// Serializes an entity to bytes.
    ///
    /// Default implementation uses JSON. Override this method to use
    /// bincode or other serialization formats.
    ///
    /// ## Example Override for Bincode
    ///
    /// ```rust,ignore
    /// fn serialize(&self, entity: &V) -> Result<Vec<u8>> {
    ///     bincode::serialize(entity)
    ///         .map_err(|e| StorageError::SerializationError(e.to_string()))
    /// }
    /// ```
    fn serialize(&self, entity: &V) -> Result<Vec<u8>> {
        serde_json::to_vec(entity).map_err(|e| StorageError::SerializationError(e.to_string()))
    }

    /// Deserializes bytes to an entity.
    ///
    /// Default implementation uses JSON. Override this method to use
    /// bincode or other serialization formats.
    fn deserialize(&self, bytes: &[u8]) -> Result<V> {
        serde_json::from_slice(bytes).map_err(|e| StorageError::SerializationError(e.to_string()))
    }

    /// Stores an entity with the given key.
    ///
    /// The entity is serialized using `serialize()` before storage.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let user_id = UserId::new("u1");
    /// let user = User { id: user_id.clone(), ... };
    /// store.put(&user_id, &user)?;
    /// ```
    fn put(&self, key: &K, entity: &V) -> Result<()> {
        let partition = Partition::new(self.partition());
        let value = self.serialize(entity)?;
        self.backend().put(&partition, &key.storage_key(), &value)
    }

    /// Stores multiple entities atomically in a batch.
    ///
    /// All writes succeed or all fail (atomic operation). This is 100× faster
    /// than individual `put()` calls for bulk inserts (e.g., Parquet restore).
    ///
    /// ## Performance
    ///
    /// - Individual puts: 1000 rows = 1000 RocksDB writes (~100ms)
    /// - Batch put: 1000 rows = 1 RocksDB write batch (~1ms)
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let entries = vec![
    ///     (UserId::new("u1"), User { ... }),
    ///     (UserId::new("u2"), User { ... }),
    ///     (UserId::new("u3"), User { ... }),
    /// ];
    /// store.batch_put(&entries)?; // Atomic write
    /// ```
    fn batch_put(&self, entries: &[(K, V)]) -> Result<()> {
        use crate::storage_trait::Operation;

        let partition = Partition::new(self.partition());
        let operations: Result<Vec<Operation>> = entries
            .iter()
            .map(|(key, entity)| {
                let value = self.serialize(entity)?;
                Ok(Operation::Put {
                    partition: partition.clone(),
                    key: key.storage_key(),
                    value,
                })
            })
            .collect();

        self.backend().batch(operations?)
    }

    /// Retrieves an entity by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let user_id = UserId::new("u1");
    /// if let Some(user) = store.get(&user_id)? {
    ///     println!("Found user: {}", user.name);
    /// }
    /// ```
    fn get(&self, key: &K) -> Result<Option<V>> {
        let partition = Partition::new(self.partition());
        match self.backend().get(&partition, &key.storage_key())? {
            Some(bytes) => Ok(Some(self.deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Deletes an entity by key.
    ///
    /// Returns `Ok(())` even if the key doesn't exist (idempotent).
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let user_id = UserId::new("u1");
    /// store.delete(&user_id)?;
    /// ```
    fn delete(&self, key: &K) -> Result<()> {
        let partition = Partition::new(self.partition());
        self.backend().delete(&partition, &key.storage_key())
    }

    /// Scans entities with keys matching the given prefix.
    ///
    /// Returns a vector of (key, entity) pairs. Keys are deserialized from bytes.
    ///
    /// **Note**: This method returns raw byte keys. Subtraits may provide
    /// type-safe key deserialization for specific key types.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let prefix = UserId::new("user_");
    /// let results = store.scan_prefix(&prefix)?;
    /// for (key_bytes, user) in results {
    ///     println!("User: {}", user.name);
    /// }
    /// ```
    fn scan_prefix(&self, prefix: &K) -> Result<Vec<(Vec<u8>, V)>> {
        let partition = Partition::new(self.partition());
        let iter = self
            .backend()
            .scan(&partition, Some(&prefix.storage_key()), None)?;

        let mut results = Vec::new();
        for (key_bytes, value_bytes) in iter {
            let entity = self.deserialize(&value_bytes)?;
            results.push((key_bytes, entity));
        }

        Ok(results)
    }

    /// Scans all entities in the partition.
    ///
    /// **Warning**: This loads all entities into memory. Use with caution on large datasets.
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// let all_users = store.scan_all()?;
    /// println!("Total users: {}", all_users.len());
    /// ```
    fn scan_all(&self) -> Result<Vec<(Vec<u8>, V)>> {
        // Place a hard limit to bypass scanning massive tables into memory
        const MAX_SCAN_LIMIT: usize = 100000;
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, None, None)?;

        let mut count = 0;
        let mut results = Vec::new();
        for (key_bytes, value_bytes) in iter {
            let entity = self.deserialize(&value_bytes)?;
            results.push((key_bytes, entity));
            count += 1;
            if count >= MAX_SCAN_LIMIT {
                log::warn!(
                    "Scan all reached max limit of {} entries, stopping early",
                    MAX_SCAN_LIMIT
                );
                break;
            }
        }

        Ok(results)
    }

    /// Scans entities with an overall limit and optional prefix, returning at most `limit` entities.
    ///
    /// This method streams from the underlying backend and stops early once the requested
    /// number of results is reached. Prefer this over `scan_all()` for large datasets.
    ///
    /// - When `prefix` is `None`, scans from the beginning of the partition.
    /// - When `prefix` is provided, scans keys beginning with the given byte prefix.
    ///
    /// Note: Keys are returned as raw bytes. Higher-level stores may provide typed wrappers.
    fn scan_limited_with_prefix_bytes(
        &self,
        prefix: Option<&[u8]>,
        limit: usize,
    ) -> Result<Vec<(Vec<u8>, V)>> {
        let partition = Partition::new(self.partition());
        let iter = self.backend().scan(&partition, prefix, Some(limit))?;

        let mut results = Vec::with_capacity(limit);
        for (key_bytes, value_bytes) in iter {
            let entity = self.deserialize(&value_bytes)?;
            results.push((key_bytes, entity));
            if results.len() >= limit {
                break;
            }
        }
        Ok(results)
    }

    /// Scans entities limited to `limit` results (no prefix).
    fn scan_limited(&self, limit: usize) -> Result<Vec<(Vec<u8>, V)>> {
        self.scan_limited_with_prefix_bytes(None, limit)
    }

    /// Scans entities with a byte prefix limited to `limit` results.
    fn scan_prefix_limited_bytes(&self, prefix: &[u8], limit: usize) -> Result<Vec<(Vec<u8>, V)>> {
        self.scan_limited_with_prefix_bytes(Some(prefix), limit)
    }
}

/// Trait for cross-user table stores with access control.
///
/// This trait extends `EntityStore<K, V>` to add access control functionality
/// for tables that are shared across users (system tables and shared tables).
///
/// ## Access Control Model
///
/// - **System tables** (e.g., system.users, system.jobs):
///   - `table_access()` returns `None`
///   - Only admin roles (dba, system) can read
///
/// - **Shared tables** with access levels:
///   - `table_access()` returns `Some(TableAccess::Public/Private/Restricted)`
///   - Access rules depend on table configuration
///
/// ## Example Usage
///
/// ```rust,ignore
/// use kalamdb_store::CrossUserTableStore;
/// use kalamdb_commons::models::{Role, TableAccess};
///
/// fn check_access<S: CrossUserTableStore<K, V>>(
///     store: &S,
///     user_role: &Role
/// ) -> bool {
///     store.can_read(user_role)
/// }
/// ```
pub trait CrossUserTableStore<K, V>: EntityStore<K, V>
where
    K: StorageKey,
    V: Serialize + for<'de> Deserialize<'de> + Send + Sync,
{
    /// Returns the table access level, if any.
    ///
    /// - `None`: System table (admin-only, e.g., system.users)
    /// - `Some(TableAccess::Public)`: Publicly readable
    /// - `Some(TableAccess::Private)`: Owner-only
    /// - `Some(TableAccess::Restricted)`: Service+ roles only
    fn table_access(&self) -> Option<TableAccess>;

    /// Checks if a user with the given role can read this table.
    ///
    /// ## Access Rules
    ///
    /// - **System tables** (`table_access() = None`): Only dba/system roles
    /// - **Public tables**: All roles can read
    /// - **Private tables**: No cross-user access (owner only)
    /// - **Restricted tables**: Service/dba/system roles only
    ///
    /// ## Example
    ///
    /// ```rust,ignore
    /// use kalamdb_commons::models::Role;
    ///
    /// let can_access = store.can_read(&Role::User);  // false for system tables
    /// let admin_access = store.can_read(&Role::Dba); // true for system tables
    /// ```
    fn can_read(&self, user_role: &Role) -> bool {
        match self.table_access() {
            // System tables: service and admin roles
            // Service role is allowed read-only access to system tables for operational telemetry.
            None => matches!(user_role, Role::Service | Role::Dba | Role::System),

            // Shared tables: check access level
            Some(TableAccess::Public) => true,
            Some(TableAccess::Private) => false, // Owner-only, not cross-user
            Some(TableAccess::Restricted) => {
                matches!(user_role, Role::Service | Role::Dba | Role::System)
            }
        }
    }
}

// Import Role and TableAccess for the trait
use kalamdb_commons::models::{Role, TableAccess};

#[cfg(test)]
mod tests {
    use super::*;
    use kalamdb_commons::models::{Role, TableAccess, UserId};
    use std::sync::Arc;

    // Mock implementation for testing
    struct MockStore {
        backend: Arc<dyn StorageBackend>,
        access: Option<TableAccess>,
    }

    impl EntityStore<UserId, String> for MockStore {
        fn backend(&self) -> &Arc<dyn StorageBackend> {
            &self.backend
        }

        fn partition(&self) -> &str {
            "test_partition"
        }
    }

    impl CrossUserTableStore<UserId, String> for MockStore {
        fn table_access(&self) -> Option<TableAccess> {
            self.access
        }
    }

    #[test]
    fn test_system_table_access() {
        // System table (table_access = None)
        let store = MockStore {
            backend: Arc::new(crate::test_utils::InMemoryBackend::new()),
            access: None,
        };

        assert!(!store.can_read(&Role::User)); // Regular users cannot read system tables
        assert!(store.can_read(&Role::Service)); // Service accounts CAN read for operational telemetry
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }

    #[test]
    fn test_public_table_access() {
        let store = MockStore {
            backend: Arc::new(crate::test_utils::InMemoryBackend::new()),
            access: Some(TableAccess::Public),
        };

        assert!(store.can_read(&Role::User));
        assert!(store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }

    #[test]
    fn test_private_table_access() {
        let store = MockStore {
            backend: Arc::new(crate::test_utils::InMemoryBackend::new()),
            access: Some(TableAccess::Private),
        };

        assert!(!store.can_read(&Role::User));
        assert!(!store.can_read(&Role::Service));
        assert!(!store.can_read(&Role::Dba));
        assert!(!store.can_read(&Role::System));
    }

    #[test]
    fn test_restricted_table_access() {
        let store = MockStore {
            backend: Arc::new(crate::test_utils::InMemoryBackend::new()),
            access: Some(TableAccess::Restricted),
        };

        assert!(!store.can_read(&Role::User));
        assert!(store.can_read(&Role::Service));
        assert!(store.can_read(&Role::Dba));
        assert!(store.can_read(&Role::System));
    }
}
