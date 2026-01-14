//! system.cluster_groups virtual view
//!
//! **Type**: Virtual View (not backed by persistent storage)
//!
//! Provides per-Raft-group membership and replication status.
//! Pattern mirrors `system.cluster` (see `views/cluster.rs`).

use super::view_base::{ViewTableProvider, VirtualView};
use crate::schema_registry::RegistryError;
use datafusion::arrow::array::{
    ArrayRef, BooleanArray, StringArray, UInt64Array,
};
use datafusion::arrow::datatypes::SchemaRef;
use datafusion::arrow::record_batch::RecordBatch;
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::schemas::{
    ColumnDefault, ColumnDefinition, TableDefinition, TableOptions, TableType,
};
use kalamdb_commons::{NamespaceId, SystemTable, TableName};
use kalamdb_raft::{CommandExecutor, GroupId};
use serde_json::json;
use std::sync::{Arc, OnceLock};

#[derive(Debug, Clone, Copy)]
enum ClusterGroupType {
    Meta,
    UserData,
    SharedData,
}

impl ClusterGroupType {
    fn as_str(&self) -> &'static str {
        match self {
            ClusterGroupType::Meta => "meta",
            ClusterGroupType::UserData => "user_data",
            ClusterGroupType::SharedData => "shared_data",
        }
    }
}

fn cluster_groups_schema() -> SchemaRef {
    static SCHEMA: OnceLock<SchemaRef> = OnceLock::new();
    SCHEMA
        .get_or_init(|| {
            ClusterGroupsView::definition()
                .to_arrow_schema()
                .expect("Failed to convert cluster_groups TableDefinition to Arrow schema")
        })
        .clone()
}

#[derive(Debug)]
pub struct ClusterGroupsView {
    executor: Arc<dyn CommandExecutor>,
}

impl ClusterGroupsView {
    pub fn definition() -> TableDefinition {
        let columns = vec![
            ColumnDefinition::new(
                1,
                "cluster_id",
                1,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Cluster identifier".to_string()),
            ),
            ColumnDefinition::new(
                2,
                "group_id",
                2,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Raft group numeric ID".to_string()),
            ),
            ColumnDefinition::new(
                3,
                "group_type",
                3,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Group type (meta, user_data, shared_data)".to_string()),
            ),
            ColumnDefinition::new(
                4,
                "node_id",
                4,
                KalamDataType::BigInt,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node ID".to_string()),
            ),
            ColumnDefinition::new(
                5,
                "role",
                5,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Node role for this group (leader, follower, learner, candidate, shutdown)".to_string()),
            ),
            ColumnDefinition::new(
                6,
                "is_leader",
                6,
                KalamDataType::Boolean,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Whether this node is leader for this group".to_string()),
            ),
            ColumnDefinition::new(
                7,
                "term",
                7,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Current Raft term (nullable if metrics unavailable)".to_string()),
            ),
            ColumnDefinition::new(
                8,
                "last_applied",
                8,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Last applied log index (nullable if metrics unavailable)".to_string()),
            ),
            ColumnDefinition::new(
                9,
                "leader_node_id",
                9,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Leader node id for this group (nullable if unknown)".to_string()),
            ),
            ColumnDefinition::new(
                10,
                "membership",
                10,
                KalamDataType::Text,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("JSON membership: voters + learners".to_string()),
            ),
            ColumnDefinition::new(
                11,
                "replication_lag",
                11,
                KalamDataType::BigInt,
                true,
                false,
                false,
                ColumnDefault::None,
                Some("Replication lag (entries behind leader), NULL on leader/unknown".to_string()),
            ),
            ColumnDefinition::new(
                12,
                "snapshot_state",
                12,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Snapshot state: none | streaming | installing".to_string()),
            ),
            ColumnDefinition::new(
                13,
                "status",
                13,
                KalamDataType::Text,
                false,
                false,
                false,
                ColumnDefault::None,
                Some("Group status: active | initializing | removed".to_string()),
            ),
        ];

        TableDefinition::new(
            NamespaceId::system(),
            TableName::new(SystemTable::ClusterGroups.table_name()),
            TableType::System,
            columns,
            TableOptions::system(),
            Some("Per-Raft-group membership and replication status (read-only view)".to_string()),
        )
        .expect("Failed to create system.cluster_groups view definition")
    }

    pub fn new(executor: Arc<dyn CommandExecutor>) -> Self {
        Self { executor }
    }

    fn group_status_str(metrics_available: bool) -> &'static str {
        if metrics_available {
            "active"
        } else {
            "initializing"
        }
    }

    fn snapshot_state_str(_metrics_available: bool) -> &'static str {
        // KalamDB doesn't currently expose streaming/installing state.
        "none"
    }
}

impl VirtualView for ClusterGroupsView {
    fn system_table(&self) -> SystemTable {
        SystemTable::ClusterGroups
    }

    fn schema(&self) -> SchemaRef {
        cluster_groups_schema()
    }

    fn compute_batch(&self) -> Result<RecordBatch, RegistryError> {
        let info = self.executor.get_cluster_info();

        // Try to downcast to RaftExecutor so we can read per-group OpenRaft metrics.
        // If downcast fails, we still emit rows but with NULL metrics.
        let raft_exec = self.executor.as_any().downcast_ref::<kalamdb_raft::RaftExecutor>();

        let group_ids = if let Some(re) = raft_exec {
            re.manager().all_group_ids()
        } else {
            let mut ids = Vec::with_capacity(info.total_groups as usize);
            ids.push(GroupId::Meta);
            for shard in 0..info.user_shards {
                ids.push(GroupId::DataUserShard(shard));
            }
            for shard in 0..info.shared_shards {
                ids.push(GroupId::DataSharedShard(shard));
            }
            ids
        };

        // Pre-allocate with an estimate: groups * nodes
        let estimated_rows = (group_ids.len().max(1)) * (info.nodes.len().max(1));

        let mut cluster_ids: Vec<&str> = Vec::with_capacity(estimated_rows);
        let mut group_id_nums: Vec<u64> = Vec::with_capacity(estimated_rows);
        let mut group_types: Vec<&str> = Vec::with_capacity(estimated_rows);
        let mut node_ids: Vec<u64> = Vec::with_capacity(estimated_rows);
        let mut roles: Vec<&str> = Vec::with_capacity(estimated_rows);
        let mut is_leaders: Vec<bool> = Vec::with_capacity(estimated_rows);

        let mut terms: Vec<Option<u64>> = Vec::with_capacity(estimated_rows);
        let mut last_applieds: Vec<Option<u64>> = Vec::with_capacity(estimated_rows);
        let mut leader_node_ids: Vec<Option<u64>> = Vec::with_capacity(estimated_rows);
        let mut memberships_json: Vec<Option<String>> = Vec::with_capacity(estimated_rows);
        let mut replication_lags: Vec<Option<u64>> = Vec::with_capacity(estimated_rows);

        let mut snapshot_states: Vec<&str> = Vec::with_capacity(estimated_rows);
        let mut statuses: Vec<&str> = Vec::with_capacity(estimated_rows);

        for gid in group_ids {
            let group_type = match gid {
                GroupId::Meta => ClusterGroupType::Meta,
                GroupId::DataUserShard(_) => ClusterGroupType::UserData,
                GroupId::DataSharedShard(_) => ClusterGroupType::SharedData,
            };

            let metrics_opt = raft_exec.and_then(|re| re.manager().group_metrics(gid));

            if let Some(metrics) = metrics_opt {
                let leader_id = metrics.current_leader;
                let is_self_leader = leader_id == Some(metrics.id);
                let voters: std::collections::BTreeSet<u64> =
                    metrics.membership_config.voter_ids().collect();

                let mut all_node_ids: Vec<u64> = Vec::new();
                for (node_id, _node) in metrics.membership_config.nodes() {
                    all_node_ids.push(*node_id);
                }

                let learners: Vec<u64> = all_node_ids
                    .iter()
                    .copied()
                    .filter(|id| !voters.contains(id))
                    .collect();
                let voters_vec: Vec<u64> = voters.iter().copied().collect();
                let membership = json!({
                    "voters": voters_vec,
                    "learners": learners,
                })
                .to_string();

                for node_id in all_node_ids {
                    cluster_ids.push(info.cluster_id.as_str());
                    group_id_nums.push(gid.as_u64());
                    group_types.push(group_type.as_str());
                    node_ids.push(node_id);

                    let is_leader = leader_id == Some(node_id);
                    is_leaders.push(is_leader);

                    // Keep role values aligned with the requested set: leader | follower | learner
                    let role = if is_leader {
                        "leader"
                    } else if voters.contains(&node_id) {
                        "follower"
                    } else {
                        "learner"
                    };
                    roles.push(role);

                    terms.push(Some(metrics.current_term));
                    leader_node_ids.push(leader_id);

                    // last_applied: for self use metrics.last_applied; for others use replication match (leader only)
                    if node_id == metrics.id {
                        last_applieds.push(metrics.last_applied.map(|l| l.index));
                    } else if is_self_leader {
                        let applied = metrics
                            .replication
                            .as_ref()
                            .and_then(|rep| rep.get(&node_id))
                            .and_then(|matched| matched.map(|l| l.index));
                        last_applieds.push(applied);
                    } else {
                        last_applieds.push(None);
                    }

                    memberships_json.push(Some(membership.clone()));

                    // replication_lag: only available on leader; NULL on leader row by requirement
                    if is_leader {
                        replication_lags.push(None);
                    } else if is_self_leader {
                        if let (Some(rep), Some(last_log_idx)) = (metrics.replication.as_ref(), metrics.last_log_index) {
                            let matched_idx = rep
                                .get(&node_id)
                                .and_then(|matched| matched.map(|l| l.index))
                                .unwrap_or(0);
                            replication_lags.push(Some(last_log_idx.saturating_sub(matched_idx)));
                        } else {
                            replication_lags.push(None);
                        }
                    } else {
                        replication_lags.push(None);
                    }

                    snapshot_states.push(Self::snapshot_state_str(true));
                    statuses.push(Self::group_status_str(true));
                }
            } else {
                // Fallback: emit one row per known node with unknown per-group metrics.
                for node in &info.nodes {
                    cluster_ids.push(info.cluster_id.as_str());
                    group_id_nums.push(gid.as_u64());
                    group_types.push(group_type.as_str());
                    node_ids.push(node.node_id.as_u64());

                    is_leaders.push(false);
                    roles.push("follower");

                    terms.push(None);
                    last_applieds.push(None);
                    leader_node_ids.push(None);
                    memberships_json.push(None);
                    replication_lags.push(None);

                    snapshot_states.push(Self::snapshot_state_str(false));
                    statuses.push(Self::group_status_str(false));
                }
            }
        }

        RecordBatch::try_new(
            self.schema(),
            vec![
                Arc::new(StringArray::from(cluster_ids)) as ArrayRef,
                Arc::new(UInt64Array::from(group_id_nums)) as ArrayRef,
                Arc::new(StringArray::from(group_types)) as ArrayRef,
                Arc::new(UInt64Array::from(node_ids)) as ArrayRef,
                Arc::new(StringArray::from(roles)) as ArrayRef,
                Arc::new(BooleanArray::from(is_leaders)) as ArrayRef,
                Arc::new(UInt64Array::from(terms)) as ArrayRef,
                Arc::new(UInt64Array::from(last_applieds)) as ArrayRef,
                Arc::new(UInt64Array::from(leader_node_ids)) as ArrayRef,
                {
                    let refs: Vec<Option<&str>> = memberships_json
                        .iter()
                        .map(|s| s.as_deref())
                        .collect();
                    Arc::new(StringArray::from(refs)) as ArrayRef
                },
                Arc::new(UInt64Array::from(replication_lags)) as ArrayRef,
                Arc::new(StringArray::from(snapshot_states)) as ArrayRef,
                Arc::new(StringArray::from(statuses)) as ArrayRef,
            ],
        )
        .map_err(|e| RegistryError::Other(format!("Failed to build cluster_groups batch: {}", e)))
    }
}

pub type ClusterGroupsTableProvider = ViewTableProvider<ClusterGroupsView>;

pub fn create_cluster_groups_provider(
    executor: Arc<dyn CommandExecutor>,
) -> ClusterGroupsTableProvider {
    ViewTableProvider::new(Arc::new(ClusterGroupsView::new(executor)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_schema() {
        let schema = cluster_groups_schema();
        assert_eq!(schema.fields().len(), 13);
        assert_eq!(schema.field(0).name(), "cluster_id");
        assert_eq!(schema.field(1).name(), "group_id");
        assert_eq!(schema.field(2).name(), "group_type");
        assert_eq!(schema.field(3).name(), "node_id");
        assert_eq!(schema.field(4).name(), "role");
    }
}
