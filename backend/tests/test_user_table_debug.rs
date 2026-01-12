//! Minimal test to debug USER table issue

#[path = "common/testserver/mod.rs"]
mod test_support;

use test_support::http_server::with_http_test_server_timeout;
use tokio::time::Duration;
use kalamdb_commons::Role;

#[tokio::test]
async fn test_user_table_insert_select_minimal() {
    with_http_test_server_timeout(Duration::from_secs(60), |server| {
        Box::pin(async move {
            println!("\n=== Starting minimal USER table test ===\n");
            
            // Create namespace
            let ns = "test_ns_minimal";
            let resp = server.execute_sql(&format!("CREATE NAMESPACE {}", ns)).await?;
            println!("CREATE NAMESPACE: status={:?}", resp.status);
            assert!(resp.success(), "CREATE NAMESPACE failed: {:?}", resp.error);
            
            // Create USER table
            let create_sql = format!(
                "CREATE TABLE {}.messages (id INT PRIMARY KEY, content TEXT) WITH (TYPE='USER', STORAGE_ID='local')",
                ns
            );
            let resp = server.execute_sql(&create_sql).await?;
            println!("CREATE TABLE: status={:?}", resp.status);
            assert!(resp.success(), "CREATE TABLE failed: {:?}", resp.error);
            
            // Create user and get client
            let user_sql = "CREATE USER 'testuser' WITH PASSWORD 'pass123' ROLE 'user'";
            let resp = server.execute_sql(user_sql).await?;
            println!("CREATE USER: status={:?} error={:?}", resp.status, resp.error);
            
            // Get user_id
            let resp = server.execute_sql("SELECT user_id, username FROM system.users WHERE username='testuser'").await?;
            println!("SELECT user_id: status={:?}", resp.status);
            let rows = resp.rows_as_maps();
            println!("Users found: {:?}", rows);
            
            let user_id = rows.first()
                .and_then(|r| r.get("user_id"))
                .and_then(|v| v.as_str())
                .unwrap_or("unknown");
            println!("User ID: {}", user_id);
            
            // Create link client for this user
            let client = server.link_client_with_id(user_id, "testuser", &Role::User);
            
            // INSERT via link client
            let insert_sql = format!("INSERT INTO {}.messages (id, content) VALUES (1, 'test message')", ns);
            let resp = client.execute_query(&insert_sql, None, None).await?;
            println!("INSERT: success={} error={:?}", resp.success(), resp.error);
            assert!(resp.success(), "INSERT failed: {:?}", resp.error);
            
            // SELECT via link client
            let select_sql = format!("SELECT COUNT(*) as cnt FROM {}.messages", ns);
            let resp = client.execute_query(&select_sql, None, None).await?;
            println!("SELECT COUNT: success={} error={:?}", resp.success(), resp.error);
            
            if resp.success() {
                let rows = resp.rows_as_maps();
                println!("Query result rows: {:?}", rows);
                let count = resp.get_i64("cnt").unwrap_or(-1);
                println!("Count value: {}", count);
                assert_eq!(count, 1, "Expected 1 row, got {}", count);
            } else {
                panic!("SELECT failed: {:?}", resp.error);
            }
            
            println!("\n=== Test completed successfully ===\n");
            Ok(())
        })
    })
    .await
    .expect("Test failed");
}
