use crate::error::Result;
use kalamdb_commons::ids::StreamTableRowId;
use kalamdb_commons::models::{StreamTableRow, TableId, UserId};
use std::collections::HashMap;

/// Stream log storage trait.
pub trait StreamLogStore: Send + Sync {
    fn append_rows(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        rows: HashMap<StreamTableRowId, StreamTableRow>,
    ) -> Result<()>;
    fn read_with_limit(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        limit: usize,
    ) -> Result<HashMap<StreamTableRowId, StreamTableRow>>;
    fn read_in_time_range(
        &self,
        table_id: &TableId,
        user_id: &UserId,
        start_time: u64,
        end_time: u64,
        limit: usize,
    ) -> Result<HashMap<StreamTableRowId, StreamTableRow>>;
    fn delete_old_logs(&self, before_time: u64) -> Result<()>;
}
