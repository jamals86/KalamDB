//! RaftExecutor - Cluster mode executor using Raft consensus
//!
//! This executor routes commands through Raft groups for consensus
//! before applying them to the local state machine.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;

use async_trait::async_trait;
use openraft::ServerState;

use kalamdb_commons::models::{NodeId, UserId};
use kalamdb_sharding::ShardRouter;

use crate::cluster_types::NodeStatus;
use crate::{
    manager::RaftManager, ClusterInfo, ClusterNodeInfo, CommandExecutor, DataResponse, GroupId,
    KalamNode, MetaCommand, MetaResponse, RaftError, SharedDataCommand, UserDataCommand,
};

/// Result type for executor operations
type Result<T> = std::result::Result<T, RaftError>;

/// Cluster mode executor using Raft consensus.
///
/// Routes commands through the appropriate Raft group leader,
/// waits for consensus, then returns the result.
#[derive(Debug)]
pub struct RaftExecutor {
    /// Reference to the Raft manager
    manager: Arc<RaftManager>,
}

impl RaftExecutor {
    /// Create a new RaftExecutor with a RaftManager.
    pub fn new(manager: Arc<RaftManager>) -> Self {
        Self { manager }
    }

    /// Get a reference to the underlying RaftManager
    ///
    /// This is used during AppContext initialization to wire up appliers
    /// after the full context is created.
    pub fn manager(&self) -> &Arc<RaftManager> {
        &self.manager
    }

    /// Compute the shard for a user based on their ID
    fn user_shard(&self, user_id: &UserId) -> u32 {
        let router = ShardRouter::new(
            self.manager.config().user_shards,
            self.manager.config().shared_shards,
        );
        router.user_shard_id(user_id)
    }
}

#[async_trait]
impl CommandExecutor for RaftExecutor {
    async fn execute_meta(&self, cmd: MetaCommand) -> Result<MetaResponse> {
        self.manager.propose_meta(cmd).await
    }

    async fn execute_user_data(
        &self,
        user_id: &UserId,
        mut cmd: UserDataCommand,
    ) -> Result<DataResponse> {
        // DML commands (INSERT/UPDATE/DELETE) don't need Meta watermark waiting.
        // The table's existence and schema were validated BEFORE building the command.
        // Raft ordering guarantees DDL (CREATE TABLE) is applied before subsequent DML.
        //
        // Only set non-zero watermark for operations that explicitly depend on Meta state
        // (e.g., after DDL changes). Pure DML operations use 0 for better performance.
        //
        // See spec 021 section 5.4.1 "Watermark Nuance" for detailed analysis.
        cmd.set_required_meta_index(0);

        let shard = self.user_shard(user_id);
        let response = self.manager.propose_user_data(shard, cmd).await?;

        // Check if the response is an error and convert to RaftError
        // Use Internal instead of Provider since the message already contains full context
        if let DataResponse::Error { message } = response {
            return Err(RaftError::Internal(message));
        }

        Ok(response)
    }

    async fn execute_shared_data(&self, mut cmd: SharedDataCommand) -> Result<DataResponse> {
        // DML commands (INSERT/UPDATE/DELETE) don't need Meta watermark waiting.
        // The table's existence and schema were validated BEFORE building the command.
        // Raft ordering guarantees DDL (CREATE TABLE) is applied before subsequent DML.
        //
        // See spec 021 section 5.4.1 "Watermark Nuance" for detailed analysis.
        cmd.set_required_meta_index(0);

        let router = ShardRouter::new(
            self.manager.config().user_shards,
            self.manager.config().shared_shards,
        );
        let response = self
            .manager
            .propose_shared_data(router.shared_shard_id(), cmd)
            .await?;

        // Check if the response is an error and convert to RaftError
        // Use Internal instead of Provider since the message already contains full context
        if let DataResponse::Error { message } = response {
            return Err(RaftError::Internal(message));
        }

        Ok(response)
    }

    async fn is_leader(&self, group: GroupId) -> bool {
        self.manager.is_leader(group)
    }

    async fn get_leader(&self, group: GroupId) -> Option<NodeId> {
        self.manager.current_leader(group)
    }

    fn is_cluster_mode(&self) -> bool {
        true
    }

    fn node_id(&self) -> NodeId {
        self.manager.node_id()
    }

    fn get_cluster_info(&self) -> ClusterInfo {
        let config = self.manager.config();
        let all_groups = self.manager.all_group_ids();
        let total_groups = all_groups.len() as u32;

        // Count how many groups this node leads
        let mut self_groups_leading = 0;
        for group in &all_groups {
            if self.manager.is_leader(*group) {
                self_groups_leading += 1;
            }
        }

        let meta_metrics = self.manager.meta_metrics();
        let mut voter_ids = BTreeSet::new();
        let mut nodes_map: BTreeMap<u64, KalamNode> = BTreeMap::new();

        // Extract metrics from OpenRaft
        let (
            leader_id,
            current_term,
            last_log_index,
            last_applied,
            millis_since_quorum_ack,
            replication_metrics,
            self_state,
        ) = if let Some(metrics) = meta_metrics.as_ref() {
            voter_ids.extend(metrics.membership_config.voter_ids());
            for (node_id, node) in metrics.membership_config.nodes() {
                nodes_map.insert(*node_id, node.clone());
            }

            (
                metrics.current_leader,
                metrics.current_term,
                metrics.last_log_index,
                metrics.last_applied.map(|log_id| log_id.index),
                metrics.millis_since_quorum_ack,
                metrics.replication.clone(),
                metrics.state,
            )
        } else {
            // Fallback to config when metrics not available
            // Use auto-detected metadata for self node
            nodes_map.insert(
                config.node_id.as_u64(),
                KalamNode::with_auto_metadata(
                    config.rpc_addr.clone(),
                    config.api_addr.clone(),
                ),
            );
            for peer in &config.peers {
                // Peers don't have metadata in fallback mode (will be NULL)
                nodes_map.insert(
                    peer.node_id.as_u64(),
                    KalamNode::new(
                        peer.rpc_addr.clone(),
                        peer.api_addr.clone(),
                    ),
                );
            }
            voter_ids.extend(nodes_map.keys().copied());
            (
                if self_groups_leading > 0 {
                    Some(config.node_id.as_u64())
                } else {
                    None
                },
                0,
                None,
                None,
                None,
                None,
                ServerState::Follower,
            )
        };

        // Determine self status from OpenRaft running state
        let self_status = if let Some(metrics) = meta_metrics.as_ref() {
            if metrics.running_state.is_ok() {
                NodeStatus::Active
            } else {
                NodeStatus::Offline
            }
        } else {
            NodeStatus::Unknown
        };

        // Get snapshot index for catchup progress calculation
        let snapshot_idx = if let Some(metrics) = meta_metrics.as_ref() {
            metrics.snapshot.map(|log_id| log_id.index)
        } else {
            None
        };

        // Use OpenRaft ServerState directly for self role
        let self_role = self_state;

        let mut nodes = Vec::with_capacity(nodes_map.len());
        for (node_id, node) in nodes_map {
            let is_self = node_id == config.node_id.as_u64();
            let is_leader = leader_id == Some(node_id);

            // Determine role for each node using OpenRaft ServerState
            // If not a voter, it's a learner (non-voting member)
            let role = if is_self {
                self_role
            } else if is_leader {
                ServerState::Leader
            } else if voter_ids.contains(&node_id) {
                ServerState::Follower
            } else {
                // Node is in membership but not a voter = learner
                ServerState::Learner
            };

            // Determine status and replication metrics for other nodes
            let (status, replication_lag, last_applied_log, catchup_progress_pct) = if is_self {
                (self_status, None, last_applied, None)
            } else if let Some(ref repl) = replication_metrics {
                // If we have replication metrics (leader's view), use them
                if let Some(matching) = repl.get(&node_id) {
                    let matched_index = matching.as_ref().map(|log_id| log_id.index).unwrap_or(0);
                    let leader_log_idx = last_log_index.unwrap_or(0);
                    let lag = Some(leader_log_idx.saturating_sub(matched_index));
                    let applied = Some(matched_index);

                    // Calculate catchup progress
                    // If lag > 0, node is catching up
                    let (node_status, progress) =
                        if leader_log_idx > 0 && matched_index < leader_log_idx {
                            // Calculate progress as percentage
                            let pct = if leader_log_idx > 0 {
                                ((matched_index as f64 / leader_log_idx as f64) * 100.0) as u8
                            } else {
                                100
                            };
                            (NodeStatus::CatchingUp, Some(pct))
                        } else {
                            // No lag = active
                            (NodeStatus::Active, None)
                        };

                    (node_status, lag, applied, progress)
                } else {
                    // Node in membership but no replication info yet
                    (NodeStatus::Joining, None, None, Some(0))
                }
            } else {
                // We're not the leader, so we don't have replication metrics for other nodes
                (NodeStatus::Unknown, None, None, None)
            };

            nodes.push(ClusterNodeInfo {
                node_id: NodeId::from(node_id),
                role,
                status,
                rpc_addr: node.rpc_addr.clone(),
                api_addr: node.api_addr.clone(),
                is_self,
                is_leader,
                groups_leading: if is_self { self_groups_leading } else { 0 },
                total_groups,
                current_term: Some(current_term),
                last_applied_log,
                leader_last_log_index: last_log_index,
                snapshot_index: snapshot_idx,
                catchup_progress_pct,
                millis_since_last_heartbeat: None, // TODO: heartbeat metrics are in OpenRaft 0.10+
                replication_lag,
                // Node metadata from KalamNode (replicated via membership)
                hostname: node.hostname.clone(),
                version: node.version.clone(),
                memory_mb: node.memory_mb,
                os: node.os.clone(),
                arch: node.arch.clone(),
            });
        }

        ClusterInfo {
            cluster_id: config.cluster_id.clone(),
            current_node_id: config.node_id,
            is_cluster_mode: true,
            nodes,
            total_groups,
            user_shards: config.user_shards,
            shared_shards: config.shared_shards,
            current_term,
            last_log_index,
            last_applied,
            millis_since_quorum_ack,
        }
    }

    async fn start(&self) -> Result<()> {
        // First start the RPC server so we can receive incoming Raft RPCs
        let rpc_addr = self.manager.config().rpc_addr.clone();
        crate::network::start_rpc_server(self.manager.clone(), rpc_addr).await?;

        // Then start the Raft groups
        self.manager.start().await
    }

    async fn initialize_cluster(&self) -> Result<()> {
        self.manager.initialize_cluster().await
    }

    async fn shutdown(&self) -> Result<()> {
        log::info!("RaftExecutor shutting down with graceful cluster leave...");
        self.manager.shutdown().await
    }

    fn as_any(&self) -> &dyn std::any::Any {
        self
    }
}

#[cfg(test)]
mod tests {
    // Tests require a running RaftManager, which needs network setup
    // See integration tests in kalamdb-raft/tests/
}
