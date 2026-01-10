//! Raft Network Implementation
//!
//! Provides the network transport for Raft RPCs using gRPC (tonic).

use std::collections::HashMap;
use std::sync::Arc;

use openraft::error::{InstallSnapshotError, NetworkError, RPCError, RaftError, RemoteError, Unreachable};
use openraft::network::{RaftNetwork as OpenRaftNetwork, RaftNetworkFactory as OpenRaftNetworkFactory, RPCOption};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse,
    InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use parking_lot::RwLock;
use tonic::transport::Channel;

use crate::storage::{KalamNode, KalamTypeConfig};
use crate::GroupId;

/// Simple connection error wrapper for openraft compatibility
#[derive(Debug)]
struct ConnectionError(String);

impl std::fmt::Display for ConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for ConnectionError {}

/// Network implementation for a single Raft group
pub struct RaftNetwork {
    /// Target node ID
    target: u64,
    /// Group ID for this network
    group_id: GroupId,
    /// Connect channel
    channel: Channel,
}

impl RaftNetwork {
    /// Create a new network instance
    pub fn new(target: u64, _target_node: KalamNode, group_id: GroupId, channel: Channel) -> Self {
        Self {
            target,
            group_id,
            channel,
        }
    }
    
    /// Get the gRPC channel
    fn get_channel(&self) -> Result<Channel, ConnectionError> {
        Ok(self.channel.clone())
    }
}

impl OpenRaftNetwork<KalamTypeConfig> for RaftNetwork {
    async fn append_entries(
        &mut self,
        rpc: AppendEntriesRequest<KalamTypeConfig>,
        _option: RPCOption,
    ) -> Result<AppendEntriesResponse<u64>, RPCError<u64, KalamNode, RaftError<u64>>> {
        // Get channel
        let channel = self.get_channel()
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;
        
        // Serialize request
        let request_bytes = crate::state_machine::encode(&rpc)
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        // Create gRPC request
        let mut client = crate::network::service::raft_client::RaftClient::new(channel);
        
        let grpc_request = tonic::Request::new(crate::network::service::RaftRpcRequest {
            group_id: self.group_id.to_string(),
            rpc_type: "append_entries".to_string(),
            payload: request_bytes,
        });
        
        // Send request
        let response = client.raft_rpc(grpc_request).await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        // Deserialize response
        let inner = response.into_inner();
        if !inner.error.is_empty() {
            return Err(RPCError::RemoteError(RemoteError::new(
                self.target,
                RaftError::Fatal(openraft::error::Fatal::Panicked),
            )));
        }
        
        let result: AppendEntriesResponse<u64> = crate::state_machine::decode(&inner.payload)
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        Ok(result)
    }

    async fn install_snapshot(
        &mut self,
        rpc: InstallSnapshotRequest<KalamTypeConfig>,
        _option: RPCOption,
    ) -> Result<InstallSnapshotResponse<u64>, RPCError<u64, KalamNode, RaftError<u64, InstallSnapshotError>>> {
        // Get channel
        let channel = self.get_channel()
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;
        
        // Serialize request
        let request_bytes = crate::state_machine::encode(&rpc)
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        // Create gRPC request
        let mut client = crate::network::service::raft_client::RaftClient::new(channel);
        
        let grpc_request = tonic::Request::new(crate::network::service::RaftRpcRequest {
            group_id: self.group_id.to_string(),
            rpc_type: "install_snapshot".to_string(),
            payload: request_bytes,
        });
        
        // Send request
        let response = client.raft_rpc(grpc_request).await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        // Deserialize response
        let inner = response.into_inner();
        if !inner.error.is_empty() {
            return Err(RPCError::RemoteError(RemoteError::new(
                self.target,
                RaftError::Fatal(openraft::error::Fatal::Panicked),
            )));
        }
        
        let result: InstallSnapshotResponse<u64> = crate::state_machine::decode(&inner.payload)
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        Ok(result)
    }

    async fn vote(
        &mut self,
        rpc: VoteRequest<u64>,
        _option: RPCOption,
    ) -> Result<VoteResponse<u64>, RPCError<u64, KalamNode, RaftError<u64>>> {
        // Get channel
        let channel = self.get_channel()
            .map_err(|e| RPCError::Unreachable(Unreachable::new(&e)))?;
        
        // Serialize request
        let request_bytes = crate::state_machine::encode(&rpc)
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        // Create gRPC request
        let mut client = crate::network::service::raft_client::RaftClient::new(channel);
        
        let grpc_request = tonic::Request::new(crate::network::service::RaftRpcRequest {
            group_id: self.group_id.to_string(),
            rpc_type: "vote".to_string(),
            payload: request_bytes,
        });
        
        // Send request
        let response = client.raft_rpc(grpc_request).await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        // Deserialize response
        let inner = response.into_inner();
        if !inner.error.is_empty() {
            return Err(RPCError::RemoteError(RemoteError::new(
                self.target,
                RaftError::Fatal(openraft::error::Fatal::Panicked),
            )));
        }
        
        let result: VoteResponse<u64> = crate::state_machine::decode(&inner.payload)
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;
        
        Ok(result)
    }
}

/// Factory for creating network instances
#[derive(Clone)]
pub struct RaftNetworkFactory {
    /// Group ID for this factory
    group_id: GroupId,
    /// Known nodes in the cluster
    nodes: Arc<RwLock<HashMap<u64, KalamNode>>>,
    /// Cached gRPC channels (node_id -> channel)
    channels: Arc<dashmap::DashMap<u64, Channel>>,
}

impl RaftNetworkFactory {
    /// Create a new network factory
    pub fn new(group_id: GroupId) -> Self {
        Self {
            group_id,
            nodes: Arc::new(RwLock::new(HashMap::new())),
            channels: Arc::new(dashmap::DashMap::new()),
        }
    }
    
    /// Register a node in the cluster
    pub fn register_node(&self, node_id: u64, node: KalamNode) {
        let mut nodes = self.nodes.write();
        nodes.insert(node_id, node);
    }
    
    /// Remove a node from the cluster
    pub fn unregister_node(&self, node_id: u64) {
        let mut nodes = self.nodes.write();
        nodes.remove(&node_id);
        self.channels.remove(&node_id);
    }
    
    /// Get node info by node ID (for leader forwarding)
    pub fn get_node(&self, node_id: u64) -> Option<KalamNode> {
        let nodes = self.nodes.read();
        nodes.get(&node_id).cloned()
    }
}

impl OpenRaftNetworkFactory<KalamTypeConfig> for RaftNetworkFactory {
    type Network = RaftNetwork;

    async fn new_client(&mut self, target: u64, node: &KalamNode) -> Self::Network {
        // Register the node if not already known
        self.register_node(target, node.clone());
        
        // Get or create channel
        let channel = if let Some(ch) = self.channels.get(&target) {
            ch.clone()
        } else {
            // Need to create a new channel
            // Note: Tonic's connect is async, but we can't easily do async inside dashmap entry
            // So we do it outside. This might race but it's fine (last one wins)
            
            // Note: We use the endpoint from the provided node info
            let endpoint = format!("http://{}", node.rpc_addr);
            
            // Create a channel that lazily connects
            // This avoids making a connection just to check if it works
            let ch = Channel::from_shared(endpoint)
                .expect("Invalid URI")
                .connect_timeout(std::time::Duration::from_secs(5))
                .timeout(std::time::Duration::from_secs(30))
                .connect_lazy();
                
            self.channels.insert(target, ch.clone());
            ch
        };
        
        RaftNetwork::new(target, node.clone(), self.group_id, channel)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_network_factory_creation() {
        let factory = RaftNetworkFactory::new(GroupId::Meta);
        
        factory.register_node(1, KalamNode {
            rpc_addr: "127.0.0.1:9000".to_string(),
            api_addr: "127.0.0.1:8080".to_string(),
        });
        
        let nodes = factory.nodes.read();
        assert!(nodes.contains_key(&1));
    }
}
