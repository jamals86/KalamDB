//! Storage key trait for type-safe key serialization
//!
//! This trait ensures that all keys used with SystemTableStore provide
//! a consistent and correct method for serialization to bytes.
//!
//! # Design Rationale
//!
//! Previously, SystemTableStore relied on `AsRef<[u8]>` for key serialization.
//! This caused bugs with composite keys (e.g., TableId, UserRowId) where
//! `AsRef<[u8]>` returned only the first component instead of the full
//! composite key (e.g., `b"namespace"` instead of `b"namespace:table"`).
//!
//! The StorageKey trait provides an explicit contract for storage serialization,
//! separate from AsRef which may be used for other purposes.

/// Trait for keys that can be serialized for storage in EntityStore
///
/// All keys used with SystemTableStore must implement this trait to ensure
/// correct serialization to bytes for RocksDB/Parquet storage.
///
/// # Examples
///
/// Composite key (TableId):
/// ```rust,ignore
/// impl StorageKey for TableId {
///     fn storage_key(&self) -> Vec<u8> {
///         self.as_storage_key() // Returns b"{namespace}:{table}"
///     }
/// }
/// ```
///
/// Simple key (UserId):
/// ```rust,ignore
/// impl StorageKey for UserId {
///     fn storage_key(&self) -> Vec<u8> {
///         self.as_str().as_bytes().to_vec()
///     }
/// }
/// ```
pub trait StorageKey: Clone + Send + Sync + 'static {
    /// Serialize this key to bytes for storage
    ///
    /// For composite keys, this MUST return the full composite representation.
    /// For simple keys, this returns the key's byte representation.
    fn storage_key(&self) -> Vec<u8>;
}
