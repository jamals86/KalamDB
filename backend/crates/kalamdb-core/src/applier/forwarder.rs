//! Command Forwarder - Forwards commands to the leader node via gRPC
//!
//! When a follower receives a write request, it uses this to forward
//! the command to the current leader using the Raft gRPC infrastructure.
//!
//! NOTE: This module is largely redundant since `RaftManager::propose_meta()`
//! and `RaftGroup::propose_with_forward()` handle leader forwarding internally.
//! It is kept for potential future use cases where manual forwarding is needed.

use std::time::Duration;

use kalamdb_raft::network::{ClientProposalRequest, RaftClient};
use tonic::transport::Channel;

use super::applier::LeaderInfo;
use super::error::ApplierError;

/// gRPC client for forwarding commands to the leader
/// 
/// Uses the existing Raft gRPC infrastructure for leader forwarding.
/// Implements connection pooling and retry logic with exponential backoff.
pub struct CommandForwarder {
    /// Connection timeout for establishing gRPC channel
    connect_timeout: Duration,
    /// Request timeout for individual RPC calls
    request_timeout: Duration,
    /// Maximum retry attempts
    max_retries: u32,
}

impl Default for CommandForwarder {
    fn default() -> Self {
        Self::new()
    }
}

impl CommandForwarder {
    /// Create a new CommandForwarder with default settings
    pub fn new() -> Self {
        Self {
            connect_timeout: Duration::from_secs(5),
            request_timeout: Duration::from_secs(30),
            max_retries: 3,
        }
    }
    
    /// Create a CommandForwarder with custom timeouts
    pub fn with_timeouts(
        connect_timeout: Duration,
        request_timeout: Duration,
        max_retries: u32,
    ) -> Self {
        Self {
            connect_timeout,
            request_timeout,
            max_retries,
        }
    }
    
    /// Forward a serialized command to the leader via gRPC
    /// 
    /// Uses the Raft `client_proposal` RPC endpoint which is designed
    /// for forwarding proposals from followers to leaders.
    pub async fn forward_to_leader(
        &self,
        leader_info: &LeaderInfo,
        group_id: &str,
        command_data: &[u8],
    ) -> Result<Vec<u8>, ApplierError> {
        let mut last_error = None;
        let mut backoff_ms = 100u64;
        
        for attempt in 0..self.max_retries {
            if attempt > 0 {
                log::debug!(
                    "Retry attempt {} for forwarding to leader {} (backoff: {}ms)",
                    attempt + 1,
                    leader_info.node_id,
                    backoff_ms
                );
                tokio::time::sleep(Duration::from_millis(backoff_ms)).await;
                backoff_ms = (backoff_ms * 2).min(5000); // Exponential backoff, max 5s
            }
            
            match self.try_forward(leader_info, group_id, command_data).await {
                Ok(response) => return Ok(response),
                Err(e) => {
                    log::warn!(
                        "Forward attempt {} failed: {}",
                        attempt + 1,
                        e
                    );
                    last_error = Some(e);
                }
            }
        }
        
        Err(last_error.unwrap_or_else(|| {
            ApplierError::Forward("Max retries exceeded".into())
        }))
    }
    
    /// Single attempt to forward to leader
    async fn try_forward(
        &self,
        leader_info: &LeaderInfo,
        group_id: &str,
        command_data: &[u8],
    ) -> Result<Vec<u8>, ApplierError> {
        let endpoint = format!("http://{}", leader_info.address);
        
        log::debug!(
            "Forwarding command to leader {} at {} for group {}",
            leader_info.node_id,
            endpoint,
            group_id
        );
        
        // Establish gRPC channel
        let channel = Channel::from_shared(endpoint.clone())
            .map_err(|e| ApplierError::Forward(format!("Invalid leader URI: {}", e)))?
            .connect_timeout(self.connect_timeout)
            .timeout(self.request_timeout)
            .connect()
            .await
            .map_err(|e| ApplierError::Forward(format!(
                "Failed to connect to leader at {}: {}", leader_info.address, e
            )))?;
        
        let mut client = RaftClient::new(channel);
        
        // Send the proposal via gRPC
        let request = tonic::Request::new(ClientProposalRequest {
            group_id: group_id.to_string(),
            command: command_data.to_vec(),
        });
        
        let response = client.client_proposal(request).await
            .map_err(|e| ApplierError::Forward(format!(
                "gRPC error forwarding proposal: {}", e
            )))?;
        
        let inner = response.into_inner();
        
        if inner.success {
            Ok(inner.payload)
        } else if let Some(leader_hint) = inner.leader_hint {
            // Leader might have changed - caller should retry with new leader
            Err(ApplierError::ForwardToLeader {
                leader_id: leader_hint,
                leader_addr: None, // We have node_id but not address
            })
        } else {
            Err(ApplierError::Forward(inner.error))
        }
    }
}
