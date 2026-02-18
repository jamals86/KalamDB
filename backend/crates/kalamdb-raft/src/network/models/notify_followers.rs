//! Notify-followers request/response message types.
//!
//! Used for forwarding live-query change notifications from the leader to
//! follower nodes via the `ClusterService/NotifyFollowers` gRPC method.

/// Request to forward a change notification to a follower node.
///
/// The `payload` field contains a flexbuffers-serialized `ChangeNotification`.
#[derive(Clone, PartialEq, prost::Message)]
pub struct NotifyFollowersRequest {
    /// Optional user ID that owns the table (None for shared tables)
    #[prost(string, optional, tag = "1")]
    pub user_id: Option<String>,

    /// Namespace of the table
    #[prost(string, tag = "2")]
    pub table_namespace: String,

    /// Table name
    #[prost(string, tag = "3")]
    pub table_name: String,

    /// flexbuffers-serialized ChangeNotification
    #[prost(bytes = "vec", tag = "4")]
    pub payload: Vec<u8>,
}

/// Response to a notification forwarding request.
#[derive(Clone, PartialEq, prost::Message)]
pub struct NotifyFollowersResponse {
    #[prost(bool, tag = "1")]
    pub success: bool,

    #[prost(string, tag = "2")]
    pub error: String,
}
