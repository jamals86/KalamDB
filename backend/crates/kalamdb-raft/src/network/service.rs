//! Raft gRPC Service
//!
//! Provides the gRPC service for handling incoming Raft RPCs.

use tonic::{Request, Response, Status};

/// Raft RPC request message
#[derive(Clone, PartialEq, prost::Message)]
pub struct RaftRpcRequest {
    /// Raft group ID (e.g., "MetaSystem", "DataUserShard(5)")
    #[prost(string, tag = "1")]
    pub group_id: String,
    
    /// RPC type: "vote", "append_entries", "install_snapshot"
    #[prost(string, tag = "2")]
    pub rpc_type: String,
    
    /// Serialized RPC payload
    #[prost(bytes = "vec", tag = "3")]
    pub payload: Vec<u8>,
}

/// Raft RPC response message
#[derive(Clone, PartialEq, prost::Message)]
pub struct RaftRpcResponse {
    /// Serialized response payload
    #[prost(bytes = "vec", tag = "1")]
    pub payload: Vec<u8>,
    
    /// Error message if any
    #[prost(string, tag = "2")]
    pub error: String,
}

/// Generated gRPC client module
pub mod raft_client {
    use super::*;
    use tonic::codegen::*;

    /// Raft RPC client
    #[derive(Debug, Clone)]
    pub struct RaftClient<T> {
        inner: tonic::client::Grpc<T>,
    }

    impl RaftClient<tonic::transport::Channel> {
        /// Create a new client from a channel
        pub fn new(channel: tonic::transport::Channel) -> Self {
            let inner = tonic::client::Grpc::new(channel);
            Self { inner }
        }
    }

    impl<T> RaftClient<T>
    where
        T: tonic::client::GrpcService<tonic::body::BoxBody>,
        T::Error: Into<StdError> + std::fmt::Debug,
        T::ResponseBody: Body<Data = Bytes> + std::marker::Send + 'static,
        <T::ResponseBody as Body>::Error: Into<StdError> + std::marker::Send,
    {
        /// Send a Raft RPC
        pub async fn raft_rpc(
            &mut self,
            request: impl tonic::IntoRequest<RaftRpcRequest>,
        ) -> std::result::Result<tonic::Response<RaftRpcResponse>, tonic::Status> {
            self.inner
                .ready()
                .await
                .map_err(|e| tonic::Status::new(tonic::Code::Unknown, format!("Service not ready: {:?}", e)))?;
            
            let codec = tonic::codec::ProstCodec::default();
            let path = http::uri::PathAndQuery::from_static("/kalamdb.raft.Raft/RaftRpc");
            let mut req = request.into_request();
            req.extensions_mut().insert(GrpcMethod::new("kalamdb.raft.Raft", "RaftRpc"));
            self.inner.unary(req, path, codec).await
        }
    }
}

/// Generated gRPC server module
pub mod raft_server {
    use super::*;
    use tonic::codegen::*;

    /// Raft service trait
    #[async_trait::async_trait]
    pub trait Raft: std::marker::Send + std::marker::Sync + 'static {
        /// Handle a Raft RPC
        async fn raft_rpc(
            &self,
            request: tonic::Request<RaftRpcRequest>,
        ) -> std::result::Result<tonic::Response<RaftRpcResponse>, tonic::Status>;
    }

    /// Raft service server
    #[derive(Debug)]
    pub struct RaftServer<T: Raft> {
        inner: Arc<T>,
    }

    impl<T: Raft> RaftServer<T> {
        pub fn new(inner: T) -> Self {
            Self { inner: Arc::new(inner) }
        }

        pub fn from_arc(inner: Arc<T>) -> Self {
            Self { inner }
        }
    }

    impl<T: Raft> tonic::server::NamedService for RaftServer<T> {
        const NAME: &'static str = "kalamdb.raft.Raft";
    }

    impl<T, B> tonic::codegen::Service<http::Request<B>> for RaftServer<T>
    where
        T: Raft,
        B: Body + std::marker::Send + 'static,
        B::Error: Into<StdError> + std::marker::Send + 'static,
    {
        type Response = http::Response<tonic::body::BoxBody>;
        type Error = std::convert::Infallible;
        type Future = BoxFuture<Self::Response, Self::Error>;

        fn poll_ready(&mut self, _cx: &mut std::task::Context<'_>) -> std::task::Poll<std::result::Result<(), Self::Error>> {
            std::task::Poll::Ready(Ok(()))
        }

        fn call(&mut self, req: http::Request<B>) -> Self::Future {
            let inner = self.inner.clone();
            
            match req.uri().path() {
                "/kalamdb.raft.Raft/RaftRpc" => {
                    let fut = async move {
                        let mut grpc = tonic::server::Grpc::new(tonic::codec::ProstCodec::default());
                        let method = RaftRpcSvc(inner);
                        let res = grpc.unary(method, req).await;
                        Ok(res)
                    };
                    Box::pin(fut)
                }
                _ => {
                    Box::pin(async move {
                        let mut builder = http::Response::builder();
                        builder = builder.status(200).header("grpc-status", "12");
                        Ok(builder.body(tonic::body::empty_body()).unwrap())
                    })
                }
            }
        }
    }

    struct RaftRpcSvc<T: Raft>(Arc<T>);

    impl<T: Raft> tonic::server::UnaryService<RaftRpcRequest> for RaftRpcSvc<T> {
        type Response = RaftRpcResponse;
        type Future = BoxFuture<tonic::Response<Self::Response>, tonic::Status>;

        fn call(&mut self, request: tonic::Request<RaftRpcRequest>) -> Self::Future {
            let inner = self.0.clone();
            let fut = async move { inner.raft_rpc(request).await };
            Box::pin(fut)
        }
    }
}

use std::sync::Arc;
use crate::manager::RaftManager;

/// Raft gRPC service implementation
pub struct RaftService {
    /// Reference to the Raft manager
    manager: Arc<RaftManager>,
}

impl RaftService {
    /// Create a new Raft service
    pub fn new(manager: Arc<RaftManager>) -> Self {
        Self { manager }
    }
}

#[async_trait::async_trait]
impl raft_server::Raft for RaftService {
    async fn raft_rpc(
        &self,
        request: Request<RaftRpcRequest>,
    ) -> Result<Response<RaftRpcResponse>, Status> {
        let req = request.into_inner();
        
        // Parse group ID
        let group_id = req.group_id.parse::<crate::GroupId>()
            .map_err(|e| Status::invalid_argument(format!("Invalid group ID: {}", e)))?;
        
        // Route to appropriate Raft group
        let result = match req.rpc_type.as_str() {
            "vote" => {
                self.manager.handle_vote(group_id, &req.payload).await
            }
            "append_entries" => {
                self.manager.handle_append_entries(group_id, &req.payload).await
            }
            "install_snapshot" => {
                self.manager.handle_install_snapshot(group_id, &req.payload).await
            }
            _ => {
                return Err(Status::invalid_argument(format!("Unknown RPC type: {}", req.rpc_type)));
            }
        };
        
        match result {
            Ok(payload) => Ok(Response::new(RaftRpcResponse {
                payload,
                error: String::new(),
            })),
            Err(e) => Ok(Response::new(RaftRpcResponse {
                payload: Vec::new(),
                error: e.to_string(),
            })),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_request_message() {
        let req = RaftRpcRequest {
            group_id: "MetaSystem".to_string(),
            rpc_type: "vote".to_string(),
            payload: vec![1, 2, 3],
        };
        
        assert_eq!(req.group_id, "MetaSystem");
        assert_eq!(req.rpc_type, "vote");
    }
}
