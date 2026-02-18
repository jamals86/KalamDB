//! Ping request/response message types.
//!
//! Used for peer liveness checks via the `ClusterService/Ping` gRPC method.

/// Request to check cluster-peer health and reachability.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PingRequest {
    /// Node ID of the sender.
    #[prost(uint64, tag = "1")]
    pub from_node_id: u64,
}

/// Response for peer ping.
#[derive(Clone, PartialEq, prost::Message)]
pub struct PingResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,

    #[prost(string, tag = "2")]
    pub error: String,

    /// Node ID of the responding peer.
    #[prost(uint64, tag = "3")]
    pub node_id: u64,
}
