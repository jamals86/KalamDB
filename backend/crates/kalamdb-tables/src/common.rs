use kalamdb_commons::StorageKey;
use kalamdb_commons::TableId;
use kalamdb_store::entity_store::KSerializable;
use kalamdb_store::{IndexDefinition, IndexedEntityStore, Partition, StorageBackend};
use std::sync::Arc;

/// Build the canonical RocksDB partition name for a table scope.
pub fn partition_name(prefix: &str, table_id: &TableId) -> String {
    format!(
        "{}{}:{}",
        prefix,
        table_id.namespace_id().as_str(),
        table_id.table_name().as_str()
    )
}

/// Create the partition if it does not already exist. Best-effort: errors are ignored.
pub fn ensure_partition(backend: &Arc<dyn StorageBackend>, name: &str) {
    let partition = Partition::new(name.to_string());
    let _ = backend.create_partition(&partition);
}

/// Create an IndexedEntityStore after ensuring the primary partition exists.
pub fn new_indexed_store_with_pk<K, V>(
    backend: Arc<dyn StorageBackend>,
    partition: String,
    indexes: Vec<Arc<dyn IndexDefinition<K, V>>>,
) -> IndexedEntityStore<K, V>
where
    K: StorageKey,
    V: KSerializable + 'static,
{
    ensure_partition(&backend, &partition);
    IndexedEntityStore::new(backend, partition, indexes)
}
