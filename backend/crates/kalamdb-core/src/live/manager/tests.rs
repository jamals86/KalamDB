use super::*;
use crate::app_context::AppContext;
use crate::live::registry::ConnectionsManager;
use crate::live::types::ChangeNotification;
use crate::providers::arrow_json_conversion::json_to_row;
use crate::schema_registry::SchemaRegistry;
use crate::sql::executor::SqlExecutor;
use crate::test_helpers::{create_test_session, init_test_app_context};
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::models::{ConnectionId, Row, TableId};
use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
use kalamdb_commons::{NamespaceId, TableName};
use kalamdb_commons::{NodeId, UserId};
use kalamdb_store::RocksDbInit;
use kalamdb_system::providers::live_queries::LiveQueriesTableProvider;
use kalamdb_tables::{new_shared_table_store, new_stream_table_store, new_user_table_store};
use std::sync::Arc;
use std::time::Duration;
use tempfile::TempDir;

fn to_row(v: serde_json::Value) -> Row {
    json_to_row(&v).unwrap()
}

/// Helper function to create a SubscriptionRequest for tests
fn create_test_subscription_request(
    id: String,
    sql: String,
    table_id: Option<TableId>,
    last_rows: Option<u32>,
) -> kalamdb_commons::websocket::SubscriptionRequest {
    kalamdb_commons::websocket::SubscriptionRequest {
        id,
        sql,
        options: kalamdb_commons::websocket::SubscriptionOptions {
            _reserved: None,
            batch_size: None,
            last_rows,
        },
        table_id,
        where_clause: None, // Tests don't need pre-parsed WHERE clause
        projections: None,  // Tests don't need pre-parsed projections
    }
}

async fn create_test_manager() -> (Arc<ConnectionsManager>, LiveQueryManager, TempDir) {
    init_test_app_context();
    let temp_dir = TempDir::new().unwrap();
    let init = RocksDbInit::with_defaults(temp_dir.path().to_str().unwrap());
    let db = Arc::new(init.open().unwrap());
    let backend: Arc<dyn kalamdb_store::StorageBackend> =
        Arc::new(kalamdb_store::RocksDBBackend::new(Arc::clone(&db)));

    let live_queries_provider = Arc::new(LiveQueriesTableProvider::new(backend.clone()));
    let schema_registry = Arc::new(SchemaRegistry::new(128));
    let base_session_context = create_test_session();
    schema_registry.set_base_session_context(Arc::clone(&base_session_context));

    // Create table stores for testing (using default namespace and table)
    let test_namespace = NamespaceId::new("user1");
    let test_table = TableName::new("messages");
    let _user_table_store = Arc::new(new_user_table_store(
        backend.clone(),
        &test_namespace,
        &test_table,
    ));
    let _shared_table_store = Arc::new(new_shared_table_store(
        backend.clone(),
        &test_namespace,
        &test_table,
    ));
    let _stream_table_store = Arc::new(new_stream_table_store(&test_namespace, &test_table));

    // Create test table definitions via SchemaRegistry
    let messages_table = TableDefinition::new(
        NamespaceId::new("user1"),
        TableName::new("messages"),
        TableType::User,
        vec![
            ColumnDefinition::new(
                "id",
                1,
                KalamDataType::Int,
                false,
                true,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "user_id",
                2,
                KalamDataType::Text,
                true,
                false,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
        ],
        TableOptions::user(),
        None,
    )
    .unwrap();
    let messages_table_id = TableId::new(
        messages_table.namespace_id.clone(),
        messages_table.table_name.clone(),
    );
    schema_registry
        .put_table_definition(&messages_table_id, &messages_table)
        .unwrap();

    let notifications_table = TableDefinition::new(
        NamespaceId::new("user1"),
        TableName::new("notifications"),
        TableType::User,
        vec![
            ColumnDefinition::new(
                "id",
                1,
                KalamDataType::Int,
                false,
                true,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
            ColumnDefinition::new(
                "user_id",
                2,
                KalamDataType::Text,
                true,
                false,
                false,
                kalamdb_commons::schemas::ColumnDefault::None,
                None,
            ),
        ],
        TableOptions::user(),
        None,
    )
    .unwrap();
    let notifications_table_id = TableId::new(
        notifications_table.namespace_id.clone(),
        notifications_table.table_name.clone(),
    );
    schema_registry
        .put_table_definition(&notifications_table_id, &notifications_table)
        .unwrap();

    // Create connections manager first
    let connection_registry = ConnectionsManager::new(
        NodeId::from("test_node"),
        Duration::from_secs(30),
        Duration::from_secs(10),
        Duration::from_secs(5),
    );

    let manager = LiveQueryManager::new(
        live_queries_provider,
        schema_registry,
        connection_registry.clone(),
        base_session_context,
    );
    let app_ctx = AppContext::get();
    let sql_executor = Arc::new(SqlExecutor::new(app_ctx, false));
    manager.set_sql_executor(sql_executor);
    (connection_registry, manager, temp_dir)
}

/// Helper to register and authenticate a connection
fn register_and_auth_connection(
    registry: &Arc<ConnectionsManager>,
    connection_id: ConnectionId,
    user_id: UserId,
) {
    registry.register_connection(connection_id.clone(), None);
    registry.mark_authenticated(&connection_id, user_id);
}

#[test]
fn test_admin_detection_matches_sys_root() {
    // LiveQueryManager doesn't have is_admin_user method in the code I saw.
    // This test is a placeholder.
}

#[tokio::test]
async fn test_register_connection() {
    let (registry, _manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id);

    assert_eq!(registry.connection_count(), 1);
    assert_eq!(registry.subscription_count(), 0);
}

#[tokio::test]
async fn test_register_subscription() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    let table_id = TableId::from_strings("user1", "messages");
    let subscription = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages WHERE id > 0".to_string(),
        Some(table_id),
        Some(50),
    );

    let live_id = manager
        .register_subscription(connection_id.clone(), subscription)
        .await
        .unwrap();

    assert_eq!(live_id.connection_id(), &connection_id);
    assert_eq!(live_id.subscription_id(), "q1");

    assert_eq!(registry.subscription_count(), 1);
}

#[tokio::test]
async fn test_extract_table_name() {
    let (_registry, manager, _temp_dir) = create_test_manager().await;

    let table_name = manager
        .extract_table_name_from_query("SELECT * FROM user1.messages WHERE id > 0")
        .unwrap();
    assert_eq!(table_name, "user1.messages");

    let table_name = manager
        .extract_table_name_from_query("select id from test.users")
        .unwrap();
    assert_eq!(table_name, "test.users");

    let table_name = manager
        .extract_table_name_from_query("SELECT * FROM \"ns.my_table\" WHERE x = 1")
        .unwrap();
    assert_eq!(table_name, "ns.my_table");
}

#[tokio::test]
async fn test_get_subscriptions_for_table() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    let table_id1 = TableId::from_strings("user1", "messages");
    let subscription1 = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages".to_string(),
        Some(table_id1.clone()),
        None,
    );
    manager
        .register_subscription(connection_id.clone(), subscription1)
        .await
        .unwrap();

    let table_id2 = TableId::from_strings("user1", "notifications");
    let subscription2 = create_test_subscription_request(
        "q2".to_string(),
        "SELECT * FROM user1.notifications".to_string(),
        Some(table_id2.clone()),
        None,
    );
    manager
        .register_subscription(connection_id.clone(), subscription2)
        .await
        .unwrap();

    // Get subscriptions from registry (DashMap - no RwLock)
    let messages_subs = registry.get_subscriptions_for_table(&user_id, &table_id1);
    assert_eq!(messages_subs.len(), 1);
    // LiveQueryId no longer has table_id() - verify by subscription_id instead
    assert_eq!(messages_subs[0].live_id.subscription_id(), "q1");

    let notif_subs = registry.get_subscriptions_for_table(&user_id, &table_id2);
    assert_eq!(notif_subs.len(), 1);
    assert_eq!(notif_subs[0].live_id.subscription_id(), "q2");
}

#[tokio::test]
async fn test_unregister_connection() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    let table_id1 = TableId::from_strings("user1", "messages");
    let subscription1 = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages".to_string(),
        Some(table_id1),
        None,
    );
    manager
        .register_subscription(connection_id.clone(), subscription1)
        .await
        .unwrap();

    let table_id2 = TableId::from_strings("user1", "notifications");
    let subscription2 = create_test_subscription_request(
        "q2".to_string(),
        "SELECT * FROM user1.notifications".to_string(),
        Some(table_id2),
        None,
    );
    manager
        .register_subscription(connection_id.clone(), subscription2)
        .await
        .unwrap();

    let removed_live_ids = manager
        .unregister_connection(&user_id, &connection_id)
        .await
        .unwrap();
    assert_eq!(removed_live_ids.len(), 2);

    let stats = manager.get_stats().await;
    assert_eq!(stats.total_connections, 0);
    assert_eq!(stats.total_subscriptions, 0);
}

/// Test unregistering a subscription - verifies RwLock was removed correctly
/// This test previously hung forever due to an unnecessary RwLock wrapper around
/// DashMap-based LiveQueryRegistry. After removing the RwLock wrapper, this test
/// should complete quickly.
#[tokio::test]
async fn test_unregister_subscription() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    let table_id = TableId::new(
        NamespaceId::new("user1"),
        TableName::new("messages"),
    );
    let subscription_request = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages".to_string(),
        Some(table_id),
        None,
    );

    let live_id = manager
        .register_subscription(connection_id.clone(), subscription_request)
        .await
        .unwrap();

    // This should complete quickly now that RwLock wrapper has been removed
    manager.unregister_subscription(&live_id).await.unwrap();
    
    let stats = manager.get_stats().await;
    assert_eq!(stats.total_subscriptions, 0);
}

#[tokio::test]
async fn test_increment_changes() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    let table_id = TableId::from_strings("user1", "messages");
    let subscription = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages".to_string(),
        Some(table_id),
        None,
    );
    let live_id = manager
        .register_subscription(connection_id.clone(), subscription)
        .await
        .unwrap();

    manager.increment_changes(&live_id).await.unwrap();
    manager.increment_changes(&live_id).await.unwrap();

    let live_query_record = manager
        .get_live_query(&live_id.to_string())
        .await
        .unwrap()
        .unwrap();
    assert_eq!(live_query_record.changes, 2);
}

#[tokio::test]
async fn test_multi_subscription_support() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    // Multiple subscriptions on same connection
    let table_id1 = TableId::from_strings("user1", "messages");
    let subscription1 = create_test_subscription_request(
        "messages_query".to_string(),
        "SELECT * FROM user1.messages WHERE conversation_id = 'conv1'".to_string(),
        Some(table_id1),
        Some(50),
    );
    let live_id1 = manager
        .register_subscription(connection_id.clone(), subscription1)
        .await
        .unwrap();

    let table_id2 = TableId::from_strings("user1", "notifications");
    let subscription2 = create_test_subscription_request(
        "notifications_query".to_string(),
        "SELECT * FROM user1.notifications WHERE user_id = CURRENT_USER()".to_string(),
        Some(table_id2),
        Some(10),
    );
    let live_id2 = manager
        .register_subscription(connection_id.clone(), subscription2)
        .await
        .unwrap();

    let table_id3 = TableId::from_strings("user1", "messages");
    let subscription3 = create_test_subscription_request(
        "messages_query2".to_string(),
        "SELECT * FROM user1.messages WHERE conversation_id = 'conv2'".to_string(),
        Some(table_id3),
        Some(20),
    );
    let live_id3 = manager
        .register_subscription(connection_id.clone(), subscription3)
        .await
        .unwrap();

    let stats = manager.get_stats().await;
    assert_eq!(stats.total_connections, 1);
    assert_eq!(stats.total_subscriptions, 3);

    // Verify all subscriptions are tracked (DashMap - no RwLock)
    let table_id1 = TableId::from_strings("user1", "messages");
    let messages_subs = registry.get_subscriptions_for_table(&user_id, &table_id1);
    assert_eq!(messages_subs.len(), 2); // messages_query and messages_query2

    let table_id2 = TableId::from_strings("user1", "notifications");
    let notif_subs = registry.get_subscriptions_for_table(&user_id, &table_id2);
    assert_eq!(notif_subs.len(), 1);

    // Verify each has unique live_id
    assert_ne!(live_id1.to_string(), live_id2.to_string());
    assert_ne!(live_id1.to_string(), live_id3.to_string());
    assert_ne!(live_id2.to_string(), live_id3.to_string());
}

#[tokio::test]
#[ignore = "TODO: Filter compilation during subscription registration not yet implemented"]
async fn test_filter_compilation_and_caching() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    // Register subscription with WHERE clause
    let table_id = TableId::from_strings("user1", "messages");
    let subscription = create_test_subscription_request(
        "filtered_messages".to_string(),
        "SELECT * FROM user1.messages WHERE user_id = 'user1' AND read = false".to_string(),
        Some(table_id),
        None,
    );
    let live_id = manager
        .register_subscription(connection_id.clone(), subscription)
        .await
        .unwrap();

    // Verify filter was compiled and cached
    let filter_cache_arc = manager.filter_cache();
    let filter_cache = filter_cache_arc.read().await;
    let filter = filter_cache.get(&live_id.to_string());
    assert!(filter.is_some());

    // Verify filter SQL is correct (includes auto-injected user_id filter for USER tables)
    let filter = filter.unwrap();
    assert_eq!(
        filter.sql(),
        "user_id = 'user1' AND user_id = 'user1' AND read = false"
    );
}

#[tokio::test]
#[ignore = "TODO: Filter compilation during subscription registration not yet implemented"]
async fn test_notification_filtering() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    // Register subscription with filter
    let table_id = TableId::from_strings("user1", "messages");
    let subscription = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages WHERE user_id = 'user1'".to_string(),
        Some(table_id.clone()),
        None,
    );
    manager
        .register_subscription(connection_id.clone(), subscription)
        .await
        .unwrap();

    // Matching notification
    let matching_change = ChangeNotification::insert(
        TableId::from_strings("user1", "messages"),
        to_row(serde_json::json!({"user_id": "user1", "text": "Hello"})),
    );

    let table_id = TableId::from_strings("user1", "messages");
    let notified = manager
        .notify_table_change(&user_id, &table_id, matching_change)
        .await
        .unwrap();
    assert_eq!(notified, 1); // Should notify

    // Non-matching notification
    let non_matching_change = ChangeNotification::insert(
        TableId::from_strings("user1", "messages"),
        to_row(serde_json::json!({"user_id": "user2", "text": "Hello"})),
    );

    let notified = manager
        .notify_table_change(&user_id, &table_id, non_matching_change)
        .await
        .unwrap();
    assert_eq!(notified, 0); // Should NOT notify (filter didn't match)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(not(feature = "expensive_tests"), ignore)]
async fn test_filter_cleanup_on_unsubscribe() {
    let (registry, manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());
    let connection_id = ConnectionId::new("conn1".to_string());

    register_and_auth_connection(&registry, connection_id.clone(), user_id.clone());

    let table_id = TableId::from_strings("user1", "messages");
    let subscription = create_test_subscription_request(
        "q1".to_string(),
        "SELECT * FROM user1.messages WHERE user_id = 'user1'".to_string(),
        Some(table_id),
        None,
    );
    let live_id = manager
        .register_subscription(connection_id.clone(), subscription)
        .await
        .unwrap();

    // Verify filter exists
    {
        let filter_cache_arc = manager.filter_cache();
        let filter_cache = filter_cache_arc.read().await;
        assert!(filter_cache.get(&live_id.to_string()).is_some());
    }

    // Try to unregister subscription with timeout
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        manager.unregister_subscription(&live_id),
    )
    .await;

    // Filter cleanup happens first regardless of final success/failure
    match result {
        Ok(_) => {
            // Verify filter was removed (cleanup happens before DB delete)
            let filter_cache_arc = manager.filter_cache();
            let filter_cache = filter_cache_arc.read().await;
            assert!(filter_cache.get(&live_id.to_string()).is_none());
        }
        Err(_) => panic!("Test timed out after 5 seconds"),
    }
}
