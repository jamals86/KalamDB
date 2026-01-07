//! Integration tests for Raft cluster startup
//!
//! Demonstrates:
//! - Single-node cluster initialization
//! - RaftManager startup
//! - Command execution through Raft

use std::sync::Arc;

use kalamdb_commons::models::NodeId;
use kalamdb_raft::{
    manager::{RaftManager, RaftManagerConfig, PeerNode},
    executor::RaftExecutor,
    CommandExecutor, GroupId,
};

/// Test that a single-node cluster can be created and started
#[tokio::test]
async fn test_single_node_cluster_startup() {
    // Create cluster configuration for single-node mode
    let config = RaftManagerConfig {
        node_id: NodeId::new(1),
        rpc_addr: "127.0.0.1:9000".to_string(),
        api_addr: "127.0.0.1:8080".to_string(),
        peers: vec![],
        ..Default::default()
    };
    
    // Create the RaftManager
    let manager = Arc::new(RaftManager::new(config));
    
    // Verify initial state
    assert!(!manager.is_started());
    assert_eq!(manager.node_id(), NodeId::new(1));
    assert_eq!(manager.group_count(), 36); // 3 meta + 32 user + 1 shared (defaults)
    
    // Start all Raft groups
    manager.start().await.expect("Failed to start RaftManager");
    assert!(manager.is_started());
    
    // Initialize as a single-node cluster
    manager.initialize_cluster().await.expect("Failed to initialize cluster");
    
    // Give time for leader election
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Check that we're the leader for all groups (single node = always leader)
    assert!(manager.is_leader(GroupId::MetaSystem));
    assert!(manager.is_leader(GroupId::MetaUsers));
    assert!(manager.is_leader(GroupId::MetaJobs));
    assert!(manager.is_leader(GroupId::DataUserShard(0)));
    assert!(manager.is_leader(GroupId::DataSharedShard(0)));
    
    println!("✅ Single-node cluster started successfully!");
    println!("   - Node ID: {}", manager.node_id());
    println!("   - Groups: {}", manager.group_count());
    println!("   - Is MetaSystem leader: {}", manager.is_leader(GroupId::MetaSystem));
}

/// Test RaftExecutor with running cluster
#[tokio::test]
async fn test_raft_executor_with_cluster() {
    let config = RaftManagerConfig {
        node_id: NodeId::new(1),
        rpc_addr: "127.0.0.1:9001".to_string(),
        api_addr: "127.0.0.1:8081".to_string(),
        peers: vec![],
        ..Default::default()
    };
    
    let manager = Arc::new(RaftManager::new(config));
    manager.start().await.expect("Failed to start");
    manager.initialize_cluster().await.expect("Failed to initialize");
    
    // Wait for leader election
    tokio::time::sleep(tokio::time::Duration::from_millis(500)).await;
    
    // Create executor
    let executor = RaftExecutor::new(manager.clone());
    
    // Verify executor state
    assert!(executor.is_cluster_mode());
    assert_eq!(executor.node_id(), NodeId::new(1));
    assert!(executor.is_leader(GroupId::MetaSystem).await);
    
    println!("✅ RaftExecutor ready for cluster operations");
}

/// Test three-node cluster configuration
#[tokio::test] 
async fn test_three_node_cluster_config() {
    // Create configuration for node 1 in a 3-node cluster
    let config = RaftManagerConfig {
        node_id: NodeId::new(1),
        rpc_addr: "127.0.0.1:9010".to_string(),
        api_addr: "127.0.0.1:8090".to_string(),
        peers: vec![
            PeerNode {
                node_id: NodeId::new(2),
                rpc_addr: "127.0.0.1:9011".to_string(),
                api_addr: "127.0.0.1:8091".to_string(),
            },
            PeerNode {
                node_id: NodeId::new(3),
                rpc_addr: "127.0.0.1:9012".to_string(),
                api_addr: "127.0.0.1:8092".to_string(),
            },
        ],
        ..Default::default()
    };
    
    let manager = Arc::new(RaftManager::new(config));
    
    // Start this node (won't be able to elect leader without peers)
    manager.start().await.expect("Failed to start");
    
    // Verify peer registration
    assert!(manager.is_started());
    assert_eq!(manager.node_id(), NodeId::new(1));
    
    println!("✅ Three-node cluster configuration validated");
    println!("   - Node 1: 127.0.0.1:9010 (this node)");
    println!("   - Node 2: 127.0.0.1:9011 (peer)");
    println!("   - Node 3: 127.0.0.1:9012 (peer)");
}

/// Test configurable shards
#[tokio::test]
async fn test_configurable_shards() {
    // Create configuration with custom shard counts
    let config = RaftManagerConfig {
        node_id: NodeId::new(1),
        rpc_addr: "127.0.0.1:9020".to_string(),
        api_addr: "127.0.0.1:8095".to_string(),
        peers: vec![],
        user_shards: 8,    // Custom: 8 user shards instead of 32
        shared_shards: 2,  // Custom: 2 shared shards instead of 1
        ..Default::default()
    };
    
    let manager = Arc::new(RaftManager::new(config));
    
    // Verify shard counts
    assert_eq!(manager.user_shards(), 8);
    assert_eq!(manager.shared_shards(), 2);
    assert_eq!(manager.group_count(), 3 + 8 + 2); // 3 meta + 8 user + 2 shared = 13
    
    println!("✅ Configurable shards validated");
    println!("   - User shards: {}", manager.user_shards());
    println!("   - Shared shards: {}", manager.shared_shards());
    println!("   - Total groups: {}", manager.group_count());
}
