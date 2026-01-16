use crate::time_bucket::StreamTimeBucket;
use kalamdb_commons::models::TableId;
use kalamdb_sharding::ShardRouter;
use std::path::PathBuf;

/// Stream log configuration.
#[derive(Debug, Clone)]
pub struct StreamLogConfig {
    pub base_dir: PathBuf,
    pub shard_router: ShardRouter,
    pub bucket: StreamTimeBucket,
    pub table_id: TableId,
}
