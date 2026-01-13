
use super::test_support::TestServer;
use kalamdb_commons::websocket::{SubscriptionRequest, SubscriptionOptions};
use kalamdb_commons::models::{ConnectionId, UserId, ConnectionInfo};
use kalam_link::models::ResponseStatus;

#[tokio::test(flavor = "multi_thread")]
async fn test_live_queries_metadata() {
    let server = TestServer::new().await;
    let manager = server.app_context.live_query_manager();
    let registry = manager.registry();

    // 1. Register Connection
    let user_id = UserId::new("root");
    let conn_id = ConnectionId::new("conn_meta_test".to_string());
    
    let registration = registry.register_connection(conn_id.clone(), ConnectionInfo::new(None))
        .expect("Failed to register connection");
    let connection_state = registration.state;
    
    // Authenticate the connection
    connection_state.write().mark_authenticated(user_id.clone());
    registry.on_authenticated(&conn_id, user_id.clone());

    // 2. Subscribe using SubscriptionRequest
    let subscription1 = SubscriptionRequest {
        id: "sub_meta_test".to_string(),
        sql: "SELECT * FROM system.users".to_string(),
        options: SubscriptionOptions::default(),
    };
    
    println!("Registering subscription...");
    let result = manager.register_subscription_with_initial_data(
        &connection_state,
        &subscription1,
        None,
    ).await.expect("Failed to register subscription");
    let live_id = result.live_id;
    println!("Subscription registered: {}", live_id);

    // 3. Query system.live_queries via SQL
    let sql = "SELECT * FROM system.live_queries WHERE subscription_id = 'sub_meta_test'";
    println!("Executing SQL...");
    let response = server.execute_sql(sql).await;
    println!("SQL executed");
    
    assert_eq!(response.status, ResponseStatus::Success, "SQL failed: {:?}", response.error);
    let rows = response.results[0].rows.as_ref().expect("Expected rows");
    assert_eq!(rows.len(), 1, "Expected 1 row in system.live_queries");

    // 4. Unsubscribe
    let subscription_id = live_id.subscription_id().to_string();
    manager.unregister_subscription(&connection_state, &subscription_id, &live_id)
        .await
        .expect("Failed to unregister");

    // 5. Query again - should be empty
    let response_after = server.execute_sql(sql).await;
    assert_eq!(response_after.status, ResponseStatus::Success, "SQL failed: {:?}", response_after.error);
    let rows_after = response_after.results[0].rows.as_ref().expect("Expected rows");
    assert_eq!(rows_after.len(), 0, "Expected 0 rows in system.live_queries after unsubscribe");

    // 6. Test connection close cleanup
    // Re-subscribe
    let subscription2 = SubscriptionRequest {
        id: "sub_meta_test2".to_string(),
        sql: "SELECT * FROM system.users".to_string(),
        options: SubscriptionOptions::default(),
    };
    let result2 = manager.register_subscription_with_initial_data(
        &connection_state,
        &subscription2,
        None,
    ).await.expect("Failed to register subscription 2");
    let live_id2 = result2.live_id;
    println!("Second subscription registered: {}", live_id2);

    // Verify it exists
    let sql2 = "SELECT * FROM system.live_queries WHERE subscription_id = 'sub_meta_test2'";
    let response_before_close = server.execute_sql(sql2).await;
    assert_eq!(response_before_close.results[0].rows.as_ref().unwrap().len(), 1);

    // Close connection
    manager.unregister_connection(&user_id, &conn_id).await.expect("Failed to unregister connection");

    // Verify all subscriptions removed
    let response_after_close = server.execute_sql(sql2).await;
    assert_eq!(response_after_close.results[0].rows.as_ref().unwrap().len(), 0, 
        "Expected 0 rows after connection close");
}
