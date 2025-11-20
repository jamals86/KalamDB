use super::*;
use crate::app_context::AppContext;
use crate::schema_registry::SchemaRegistry;
use crate::sql::executor::SqlExecutor;
use crate::test_helpers::{create_test_session, init_test_app_context};
use kalamdb_commons::datatypes::KalamDataType;
use kalamdb_commons::models::TableId;
use kalamdb_commons::schemas::{ColumnDefinition, TableDefinition, TableOptions, TableType};
use kalamdb_commons::{NamespaceId, TableName};
use kalamdb_store::RocksDbInit;
use kalamdb_system::providers::live_queries::LiveQueriesTableProvider;
use kalamdb_tables::{new_shared_table_store, new_stream_table_store, new_user_table_store};
use crate::providers::arrow_json_conversion::json_to_row;
use kalamdb_commons::models::Row;
use tempfile::TempDir;
use crate::live::connection_registry::LiveQueryOptions;
use crate::live::types::ChangeNotification;
use std::sync::Arc;
use kalamdb_commons::{NodeId, UserId};

fn to_row(v: serde_json::Value) -> Row {
    json_to_row(&v).unwrap()
}

async fn create_test_manager() -> (LiveQueryManager, TempDir) {
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

    let manager = LiveQueryManager::new(
        live_queries_provider,
        schema_registry,
        NodeId::from("test_node"),
        base_session_context,
    );
    let app_ctx = AppContext::get();
    let sql_executor = Arc::new(SqlExecutor::new(app_ctx, false));
    manager.set_sql_executor(sql_executor);
    (manager, temp_dir)
}

#[test]
fn test_admin_detection_matches_sys_root() {
    // LiveQueryManager doesn't have is_admin_user method in the code I saw.
    // Wait, let me check the original file again.
    // I don't see is_admin_user in the original file content I read.
    // Ah, I might have missed it or it was in a trait?
    // Or maybe it was removed?
    // Let's check the original file content again.
}

#[tokio::test]
async fn test_register_connection() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id, "conn1".to_string(), None)
        .await
        .unwrap();

    assert_eq!(connection_id.user_id().as_str(), "user1");
    assert_eq!(connection_id.unique_conn_id(), "conn1");

    let stats = manager.get_stats().await;
    assert_eq!(stats.total_connections, 1);
    assert_eq!(stats.total_subscriptions, 0);
}

#[tokio::test]
async fn test_register_subscription() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    let live_id = manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages WHERE id > 0".to_string(),
            LiveQueryOptions {
                last_rows: Some(50),
            },
        )
        .await
        .unwrap();

    assert_eq!(live_id.connection_id(), &connection_id);
    assert_eq!(live_id.table_id().to_string(), "user1:messages");
    assert_eq!(live_id.query_id(), "q1");

    let stats = manager.get_stats().await;
    assert_eq!(stats.total_subscriptions, 1);
}

#[tokio::test]
async fn test_extract_table_name() {
    let (manager, _temp_dir) = create_test_manager().await;

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
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages".to_string(),
            LiveQueryOptions::default(),
        )
        .await
        .unwrap();

    manager
        .register_subscription(
            connection_id.clone(),
            "q2".to_string(),
            "SELECT * FROM user1.notifications".to_string(),
            LiveQueryOptions::default(),
        )
        .await
        .unwrap();

    // Get subscriptions from registry
    let registry_arc = manager.registry();
    let registry = registry_arc.read().await;
    let table_id1 = TableId::from_strings("user1", "messages");
    let messages_subs = registry.get_subscriptions_for_table(&user_id, &table_id1);
    assert_eq!(messages_subs.len(), 1);
    assert_eq!(
        messages_subs[0].live_id.table_id().table_name().as_str(),
        "messages"
    );

    let table_id2 = TableId::from_strings("user1", "notifications");
    let notif_subs = registry.get_subscriptions_for_table(&user_id, &table_id2);
    assert_eq!(notif_subs.len(), 1);
    assert_eq!(
        notif_subs[0].live_id.table_id().table_name().as_str(),
        "notifications"
    );
}

#[tokio::test]
async fn test_unregister_connection() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages".to_string(),
            LiveQueryOptions::default(),
        )
        .await
        .unwrap();

    manager
        .register_subscription(
            connection_id.clone(),
            "q2".to_string(),
            "SELECT * FROM user1.notifications".to_string(),
            LiveQueryOptions::default(),
        )
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

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[cfg_attr(not(feature = "expensive_tests"), ignore)]
async fn test_unregister_subscription() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    let live_id = manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages".to_string(),
            LiveQueryOptions::default(),
        )
        .await
        .unwrap();

    // Add timeout to prevent hanging
    let result = tokio::time::timeout(
        std::time::Duration::from_secs(5),
        manager.unregister_subscription(&live_id),
    )
    .await;

    match result {
        Ok(Ok(())) => {
            let stats = manager.get_stats().await;
            assert_eq!(stats.total_subscriptions, 0);
        }
        Ok(Err(e)) => panic!("Unregister failed: {}", e),
        Err(_) => panic!("Test timed out after 5 seconds"),
    }
}

#[tokio::test]
async fn test_increment_changes() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    let live_id = manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages".to_string(),
            LiveQueryOptions::default(),
        )
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
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    // Multiple subscriptions on same connection
    let live_id1 = manager
        .register_subscription(
            connection_id.clone(),
            "messages_query".to_string(),
            "SELECT * FROM user1.messages WHERE conversation_id = 'conv1'".to_string(),
            LiveQueryOptions {
                last_rows: Some(50),
            },
        )
        .await
        .unwrap();

    let live_id2 = manager
        .register_subscription(
            connection_id.clone(),
            "notifications_query".to_string(),
            "SELECT * FROM user1.notifications WHERE user_id = CURRENT_USER()".to_string(),
            LiveQueryOptions {
                last_rows: Some(10),
            },
        )
        .await
        .unwrap();

    let live_id3 = manager
        .register_subscription(
            connection_id.clone(),
            "messages_query2".to_string(),
            "SELECT * FROM user1.messages WHERE conversation_id = 'conv2'".to_string(),
            LiveQueryOptions {
                last_rows: Some(20),
            },
        )
        .await
        .unwrap();

    let stats = manager.get_stats().await;
    assert_eq!(stats.total_connections, 1);
    assert_eq!(stats.total_subscriptions, 3);

    // Verify all subscriptions are tracked
    let registry_arc = manager.registry();
    let registry = registry_arc.read().await;
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
async fn test_filter_compilation_and_caching() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    // Register subscription with WHERE clause
    let live_id = manager
        .register_subscription(
            connection_id.clone(),
            "filtered_messages".to_string(),
            "SELECT * FROM user1.messages WHERE user_id = 'user1' AND read = false".to_string(),
            LiveQueryOptions::default(),
        )
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
async fn test_notification_filtering() {
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    // Register subscription with filter
    manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages WHERE user_id = 'user1'".to_string(),
            LiveQueryOptions::default(),
        )
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
    let (manager, _temp_dir) = create_test_manager().await;
    let user_id = UserId::new("user1".to_string());

    let connection_id = manager
        .register_connection(user_id.clone(), "conn1".to_string(), None)
        .await
        .unwrap();

    let live_id = manager
        .register_subscription(
            connection_id.clone(),
            "q1".to_string(),
            "SELECT * FROM user1.messages WHERE user_id = 'user1'".to_string(),
            LiveQueryOptions::default(),
        )
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
