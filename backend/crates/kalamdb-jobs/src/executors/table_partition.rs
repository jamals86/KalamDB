use kalamdb_commons::{constants::ColumnFamilyNames, schemas::TableType, TableId};
use kalamdb_store::storage_trait::Partition;

pub(crate) fn hot_table_partition(table_type: TableType, table_id: &TableId) -> Option<Partition> {
    let partition_name = match table_type {
        TableType::User => format!("{}{}", ColumnFamilyNames::USER_TABLE_PREFIX, table_id),
        TableType::Shared => format!("{}{}", ColumnFamilyNames::SHARED_TABLE_PREFIX, table_id),
        TableType::Stream | TableType::System => return None,
    };

    Some(Partition::new(partition_name))
}
