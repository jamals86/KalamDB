#[path = "integration/common/mod.rs"]
mod common;

use common::TestServer;
use kalamdb_commons::websocket::{SubscriptionRequest, SubscriptionOptions};
use kalamdb_commons::Notification;
use kalamdb_commons::models::{ConnectionId, LiveQueryId, UserId, NamespaceId, TableName, TableId};
use tokio::sync::mpsc;

#[tokio::test(flavor = "multi_thread")]
async fn test_live_queries_metadata() {
    let server = TestServer::new().await;
    let manager = server.app_context.live_query_manager();

    // 1. Register Connection
    let user_id = UserId::new("root");
    let conn_id_str = "conn_meta_test";
    let conn_id = ConnectionId::new(conn_id_str.to_string());
    
    let (tx, _rx) = mpsc::unbounded_channel::<(LiveQueryId, Notification)>();
    
    let connection_id = manager.register_connection(
        user_id.clone(),
        conn_id.clone(),
        Some(tx)
    ).await.expect("Failed to register connection");

    // 2. Subscribe using SubscriptionRequest
    let mut subscription = SubscriptionRequest {
        id: "sub_meta_test".to_string(),
        sql: "SELECT * FROM system.users".to_string(),
        options: SubscriptionOptions::default(),
        table_id: Some(TableId::new(NamespaceId::system(), TableName::new("users"))),
        where_clause: None,
        projections: None,
    };
    
    println!("Registering subscription...");
    let live_id = manager.register_subscription(
        connection_id.clone(),
        subscription.clone(),
    ).await.expect("Failed to register subscription");
    println!("Subscription registered: {}", live_id);

    // 3. Query system.live_queries via SQL
    let sql = "SELECT * FROM system.live_queries WHERE subscription_id = 'sub_meta_test'";
    println!("Executing SQL...");
    let response = server.execute_sql(sql).await;
    println!("SQL executed");
    
    assert_eq!(response.status, kalamdb_api::models::ResponseStatus::Success, "SQL failed: {:?}", response.error);
    let rows = response.results[0].rows.as_ref().expect("Expected rows");
    assert_eq!(rows.len(), 1, "Expected 1 row in system.live_queries");

    // 4. Unsubscribe
    manager.unregister_subscription(&live_id).await.expect("Failed to unregister");

    // 5. Query again - should be empty
    let response_after = server.execute_sql(sql).await;
    assert_eq!(response_after.status, kalamdb_api::models::ResponseStatus::Success, "SQL failed: {:?}", response_after.error);
    let rows_after = response_after.results[0].rows.as_ref().expect("Expected rows");
    assert_eq!(rows_after.len(), 0, "Expected 0 rows in system.live_queries after unsubscribe");

    // 6. Test connection close cleanup
    // Re-subscribe
    subscription.id = "sub_meta_test2".to_string();
    let live_id2 = manager.register_subscription(
        connection_id.clone(),
        subscription,
    ).await.expect("Failed to register subscription 2");
    println!("Second subscription registered: {}", live_id2);

    // Verify it exists
    let sql2 = "SELECT * FROM system.live_queries WHERE subscription_id = 'sub_meta_test2'";
    let response_before_close = server.execute_sql(sql2).await;
    assert_eq!(response_before_close.results[0].rows.as_ref().unwrap().len(), 1);

    // Close connection
    manager.unregister_connection(&user_id, &connection_id).await.expect("Failed to unregister connection");

    // Verify all subscriptions removed
    let response_after_close = server.execute_sql(sql2).await;
    assert_eq!(response_after_close.results[0].rows.as_ref().unwrap().len(), 0, 
        "Expected 0 rows after connection close");
}
