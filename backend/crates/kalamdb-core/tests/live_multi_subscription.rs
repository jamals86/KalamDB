use kalamdb_commons::models::{ConnectionId, LiveQueryId, NamespaceId, TableId, TableName, UserId};
use kalamdb_commons::websocket::{SubscriptionOptions, SubscriptionRequest};
use kalamdb_commons::Notification;
use kalamdb_core::app_context::AppContext;
use kalamdb_core::test_helpers::init_test_app_context;
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn test_multi_subscription_lifecycle() {
    // 1. Setup
    let _test_db = init_test_app_context();
    let app_ctx = AppContext::get();
    let manager = app_ctx.live_query_manager();

    // 2. Register Connection
    let user_id = UserId::new("root");
    let conn_id_str = ConnectionId::new("conn_multi");
    
    let (tx, _rx) = mpsc::unbounded_channel::<(LiveQueryId, Notification)>();
    
    println!("Registering connection...");
    let connection_id = manager.register_connection(
        user_id.clone(),
        conn_id_str,
        Some(tx)
    ).await.expect("Failed to register connection");
    println!("Registered connection");

    // 3. Register Subscription 1
    let sub_id1 = "sub1";
    println!("Registering sub1...");
    let subscription1 = SubscriptionRequest {
        id: sub_id1.to_string(),
        sql: "SELECT * FROM system.users".to_string(),
        options: SubscriptionOptions::default(),
        table_id: Some(TableId::new(NamespaceId::system(), TableName::new("users"))),
        where_clause: None,
        projections: None,
    };
    let live_id1 = manager.register_subscription(
        connection_id.clone(),
        subscription1,
    ).await.expect("Failed to register sub1");
    println!("Registered sub1: {}", live_id1);

    // 4. Register Subscription 2
    let sub_id2 = "sub2";
    println!("Registering sub2...");
    let subscription2 = SubscriptionRequest {
        id: sub_id2.to_string(),
        sql: "SELECT * FROM system.jobs".to_string(),
        options: SubscriptionOptions::default(),
        table_id: Some(TableId::new(NamespaceId::system(), TableName::new("jobs"))),
        where_clause: None,
        projections: None,
    };
    let live_id2 = manager.register_subscription(
        connection_id.clone(),
        subscription2,
    ).await.expect("Failed to register sub2");
    println!("Registered sub2: {}", live_id2);

    // 5. Verify both exist
    let subs = manager.get_user_subscriptions(&user_id).await.expect("Failed to get subs");
    assert_eq!(subs.len(), 2, "Expected 2 subscriptions, got {}", subs.len());
    assert!(subs.iter().any(|s| s.subscription_id == sub_id1), "sub1 not found");
    assert!(subs.iter().any(|s| s.subscription_id == sub_id2), "sub2 not found");

    // 6. Unsubscribe Subscription 1
    println!("Unregistering sub1...");
    manager.unregister_subscription(&live_id1).await.expect("Failed to unregister sub1");
    println!("Unregistered sub1");

    // 7. Verify sub1 removed, sub2 remains
    let subs_after = manager.get_user_subscriptions(&user_id).await.expect("Failed to get subs");
    assert_eq!(subs_after.len(), 1, "Expected 1 subscription after unregister, got {}", subs_after.len());
    assert!(subs_after.iter().any(|s| s.subscription_id == sub_id2), "sub2 should remain");
    assert!(!subs_after.iter().any(|s| s.subscription_id == sub_id1), "sub1 should be removed");

    // 8. Unregister Connection
    println!("Unregistering connection...");
    manager.unregister_connection(&user_id, &connection_id).await.expect("Failed to unregister connection");
    println!("Unregistered connection");

    // 9. Verify all removed
    let subs_final = manager.get_user_subscriptions(&user_id).await.expect("Failed to get subs");
    assert_eq!(subs_final.len(), 0, "Expected 0 subscriptions after connection unregister, got {}", subs_final.len());
    
    println!("Test completed successfully!");
}
