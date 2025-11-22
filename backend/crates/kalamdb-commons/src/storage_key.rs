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
/// # Ordering Guarantees
///
/// To ensure correct range scans in RocksDB, numerical types MUST be serialized
/// in **Big-Endian** format. This preserves the natural ordering of numbers
/// when compared as byte arrays.
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
    ///
    /// **IMPORTANT**: Numerical values must be serialized using Big-Endian
    /// (e.g., `to_be_bytes()`) to ensure correct lexicographical ordering in RocksDB.
    fn storage_key(&self) -> Vec<u8>;
}

// --- Standard Implementations ---

impl StorageKey for String {
    fn storage_key(&self) -> Vec<u8> {
        self.as_bytes().to_vec()
    }
}

impl StorageKey for Vec<u8> {
    fn storage_key(&self) -> Vec<u8> {
        self.clone()
    }
}

impl StorageKey for u8 {
    fn storage_key(&self) -> Vec<u8> {
        vec![*self]
    }
}

impl StorageKey for u16 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for u32 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for u64 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for u128 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for i8 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for i16 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for i32 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for i64 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}

impl StorageKey for i128 {
    fn storage_key(&self) -> Vec<u8> {
        self.to_be_bytes().to_vec()
    }
}
