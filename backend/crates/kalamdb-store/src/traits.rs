//! Entity store trait for typed CRUD operations.
//!
//! This module provides the `EntityStore<T>` trait which sits on top of
//! `StorageBackend` to provide strongly-typed, serialized access to entities.
//!
//! ## Architecture Layers
//!
//! ```text
//! EntityStore<T>           ← Typed entity CRUD (this file)
//!     ↓
//! StorageBackend           ← Generic K/V operations (storage_trait.rs)
//!     ↓
//! RocksDB/Sled/etc         ← Actual storage implementation
//! ```
//!
//! ## Example Usage
//!
//! ```rust,ignore
//! use kalamdb_store::{EntityStore, StorageBackend, RocksDBBackend};
//! use serde::{Serialize, Deserialize};
//! use std::sync::Arc;
//!
//! #[derive(Serialize, Deserialize, Debug, Clone)]
//! struct User {
//!     id: String,
//!     name: String,
//! }
//!
//! struct UserStore {
//!     backend: Arc<dyn StorageBackend>,
//! }
//!
//! impl EntityStore<User> for UserStore {
//!     fn backend(&self) -> &Arc<dyn StorageBackend> {
//!         &self.backend
//!     }
//!     
//!     fn partition(&self) -> &str {
//!         "system_users"
//!     }
//! }
//!
//! // Now use it:
//! let user = User { id: "u1".into(), name: "Alice".into() };
//! store.put("u1", &user).unwrap();
//! let retrieved = store.get("u1").unwrap().unwrap();
//! assert_eq!(retrieved.name, "Alice");
//! ```

use crate::storage_trait::{Partition, Result, StorageBackend, StorageError};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// Trait for typed entity storage with automatic serialization.
///
/// Implementations provide:
/// - Strongly-typed CRUD operations (put, get, delete, scan)
/// - Automatic JSON serialization/deserialization (can be overridden for bincode)
/// - Partition management
///
/// ## Type Parameter
/// - `T`: Entity type that must be Serialize + Deserialize
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
pub trait EntityStore<T>
where
    T: Serialize + for<'de> Deserialize<'de>,
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
    /// fn serialize(&self, entity: &T) -> Result<Vec<u8>> {
    ///     bincode::serialize(entity)
    ///         .map_err(|e| StorageError::SerializationError(e.to_string()))
    /// }
    /// ```
    fn serialize(&self, entity: &T) -> Result<Vec<u8>> {
        serde_json::to_vec(entity).map_err(|e| StorageError::SerializationError(e.to_string()))
    }

    /// Deserializes bytes to an entity.
    ///
    /// Default implementation uses JSON. Override this method to use
    /// bincode or other serialization formats.
    fn deserialize(&self, bytes: &[u8]) -> Result<T> {
        serde_json::from_slice(bytes).map_err(|e| StorageError::SerializationError(e.to_string()))
    }

    /// Stores an entity with the given key.
    ///
    /// The entity is serialized using `serialize()` before storage.
    fn put(&self, key: &str, entity: &T) -> Result<()> {
        let partition = Partition::new(self.partition());
        let value = self.serialize(entity)?;
        self.backend().put(&partition, key.as_bytes(), &value)
    }

    /// Retrieves an entity by key.
    ///
    /// Returns `Ok(None)` if the key doesn't exist.
    fn get(&self, key: &str) -> Result<Option<T>> {
        let partition = Partition::new(self.partition());
        match self.backend().get(&partition, key.as_bytes())? {
            Some(bytes) => Ok(Some(self.deserialize(&bytes)?)),
            None => Ok(None),
        }
    }

    /// Deletes an entity by key.
    ///
    /// Returns `Ok(())` even if the key doesn't exist (idempotent).
    fn delete(&self, key: &str) -> Result<()> {
        let partition = Partition::new(self.partition());
        self.backend().delete(&partition, key.as_bytes())
    }

    /// Scans entities with keys matching the given prefix.
    ///
    /// Returns a vector of (key, entity) pairs.
    fn scan_prefix(&self, prefix: &str) -> Result<Vec<(String, T)>> {
        let partition = Partition::new(self.partition());
        let iter = self
            .backend()
            .scan(&partition, Some(prefix.as_bytes()), None)?;

        let mut results = Vec::new();
        for (key_bytes, value_bytes) in iter {
            let key = String::from_utf8(key_bytes).map_err(|e| {
                StorageError::SerializationError(format!("Invalid UTF-8 key: {}", e))
            })?;
            let entity = self.deserialize(&value_bytes)?;
            results.push((key, entity));
        }

        Ok(results)
    }

    /// Scans all entities in the partition.
    ///
    /// **Warning**: This loads all entities into memory. Use with caution on large datasets.
    fn scan_all(&self) -> Result<Vec<(String, T)>> {
        self.scan_prefix("")
    }
}

// Tests temporarily disabled - InMemoryBackend needs to be updated to use storage_trait::StorageBackend
// See index/mod.rs for working examples using RocksDBBackend
/*
#[cfg(test)]
mod tests {
    use super::*;
    use crate::test_utils::InMemoryBackend;
    use serde::{Deserialize, Serialize};

    #[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
    struct TestUser {
        id: String,
        name: String,
        age: u32,
    }

    struct TestUserStore {
        backend: Arc<dyn StorageBackend>,
    }

    impl EntityStore<TestUser> for TestUserStore {
        fn backend(&self) -> &Arc<dyn StorageBackend> {
            &self.backend
        }

        fn partition(&self) -> &str {
            "test_users"
        }
    }

    #[test]
    fn test_entity_store_put_get() {
        let backend = Arc::new(InMemoryBackend::new());
        let partition = Partition::new("test_users");
        backend.create_partition(&partition).unwrap();

        let store = TestUserStore { backend };

        let user = TestUser {
            id: "u1".to_string(),
            name: "Alice".to_string(),
            age: 30,
        };

        // Put entity
        store.put("u1", &user).unwrap();

        // Get entity
        let retrieved = store.get("u1").unwrap().unwrap();
        assert_eq!(retrieved, user);

        // Get non-existent entity
        let missing = store.get("u999").unwrap();
        assert!(missing.is_none());
    }

    #[test]
    fn test_entity_store_delete() {
        let backend = Arc::new(InMemoryBackend::new());
        let partition = Partition::new("test_users");
        backend.create_partition(&partition).unwrap();

        let store = TestUserStore { backend };

        let user = TestUser {
            id: "u1".to_string(),
            name: "Bob".to_string(),
            age: 25,
        };

        store.put("u1", &user).unwrap();
        assert!(store.get("u1").unwrap().is_some());

        store.delete("u1").unwrap();
        assert!(store.get("u1").unwrap().is_none());

        // Idempotent delete
        store.delete("u1").unwrap();
    }

    #[test]
    fn test_entity_store_scan_prefix() {
        let backend = Arc::new(InMemoryBackend::new());
        let partition = Partition::new("test_users");
        backend.create_partition(&partition).unwrap();

        let store = TestUserStore { backend };

        let users = vec![
            TestUser {
                id: "user_1".to_string(),
                name: "Alice".to_string(),
                age: 30,
            },
            TestUser {
                id: "user_2".to_string(),
                name: "Bob".to_string(),
                age: 25,
            },
            TestUser {
                id: "admin_1".to_string(),
                name: "Admin".to_string(),
                age: 40,
            },
        ];

        for user in &users {
            store.put(&user.id, user).unwrap();
        }

        // Scan with prefix
        let user_results = store.scan_prefix("user_").unwrap();
        assert_eq!(user_results.len(), 2);

        let admin_results = store.scan_prefix("admin_").unwrap();
        assert_eq!(admin_results.len(), 1);
        assert_eq!(admin_results[0].1.name, "Admin");

        // Scan all
        let all_results = store.scan_all().unwrap();
        assert_eq!(all_results.len(), 3);
    }
}
*/
