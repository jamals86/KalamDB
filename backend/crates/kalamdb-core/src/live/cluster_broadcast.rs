use std::sync::Arc;

use kalamdb_commons::cluster::LiveQueryBroadcast;
use kalamdb_commons::models::{TableId, UserId};
use kalamdb_raft::CommandExecutor;
use reqwest::Client;
use serde_json::Value;

use crate::live::types::{ChangeNotification, ChangeType};
use crate::providers::arrow_json_conversion::row_to_json_map;

pub struct ClusterLiveNotifier {
    executor: Arc<dyn CommandExecutor>,
    cluster_id: String,
    client: Client,
}

impl ClusterLiveNotifier {
    pub fn new(executor: Arc<dyn CommandExecutor>, cluster_id: String) -> Self {
        Self {
            executor,
            cluster_id,
            client: Client::new(),
        }
    }

    pub fn broadcast_change_async(
        &self,
        user_id: &UserId,
        table_id: &TableId,
        notification: &ChangeNotification,
    ) {
        if !self.executor.is_cluster_mode() {
            return;
        }

        let cluster_info = self.executor.get_cluster_info();
        let leader = cluster_info.nodes.iter().find(|node| node.is_leader);
        if leader
            .map(|node| node.node_id == cluster_info.current_node_id)
            .unwrap_or(false)
            == false
        {
            return;
        }

        let row_data = match row_to_json_map(&notification.row_data) {
            Ok(map) => Value::Object(map.into_iter().collect()),
            Err(err) => {
                log::warn!(
                    "Failed to serialize live query row for broadcast (table={}): {}",
                    table_id,
                    err
                );
                return;
            }
        };

        let old_data = match &notification.old_data {
            Some(old_row) => match row_to_json_map(old_row) {
                Ok(map) => Some(Value::Object(map.into_iter().collect())),
                Err(err) => {
                    log::warn!(
                        "Failed to serialize live query old row for broadcast (table={}): {}",
                        table_id,
                        err
                    );
                    None
                }
            },
            None => None,
        };

        let change_type = match notification.change_type {
            ChangeType::Insert => "insert",
            ChangeType::Update => "update",
            ChangeType::Delete => "delete",
            ChangeType::Flush => "flush",
        }
        .to_string();

        let payload = LiveQueryBroadcast {
            cluster_id: self.cluster_id.clone(),
            user_id: user_id.clone(),
            table_id: table_id.clone(),
            change_type,
            row_data,
            old_data,
            row_id: notification.row_id.clone(),
        };

        let client = self.client.clone();
        let cluster_id = self.cluster_id.clone();

        for node in cluster_info.nodes {
            if node.is_self {
                continue;
            }

            let url = format!(
                "{}/v1/api/cluster/live_query_notify",
                node.api_addr.trim_end_matches('/')
            );
            let payload = payload.clone();
            let client = client.clone();
            let cluster_id = cluster_id.clone();

            tokio::spawn(async move {
                let response = client
                    .post(&url)
                    .header("X-Cluster-Id", cluster_id)
                    .json(&payload)
                    .send()
                    .await;

                if let Err(err) = response {
                    log::warn!("Failed to broadcast live query change to {}: {}", url, err);
                }
            });
        }
    }
}
