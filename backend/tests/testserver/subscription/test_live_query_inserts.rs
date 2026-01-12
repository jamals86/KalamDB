//! Integration test for Live Query INSERT detection via WebSocket

#[path = "../../common/testserver/mod.rs"]
mod test_support;

use futures_util::StreamExt;
use kalam_link::models::ChangeEvent;
use kalam_link::models::ResponseStatus;
use test_support::http_server::with_http_test_server_timeout;
use tokio::time::Duration;

/// Test basic INSERT detection via live query subscription
#[tokio::test]
async fn test_live_query_detects_inserts() {
    with_http_test_server_timeout(Duration::from_secs(45), |server| {
        Box::pin(async move {
            let ns = format!("test_inserts_{}", std::process::id());
            let table = "messages";

            // Create namespace and table as root
            let resp = server
                .execute_sql(&format!("CREATE NAMESPACE {}", ns))
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            let resp = server
                .execute_sql(
                    &format!(
                        "CREATE TABLE {}.{} (
                            id TEXT PRIMARY KEY,
                            content TEXT,
                            priority INT,
                            created_at BIGINT
                        ) WITH (
                            TYPE = 'USER',
                            STORAGE_ID = 'local'
                        )",
                        ns, table
                    ),
                )
                .await?;
            assert_eq!(resp.status, ResponseStatus::Success);

            // Connect using kalam-link client
            let client = server.link_client("root");

            // Subscribe to live query
            let sql = format!("SELECT * FROM {}.{} ORDER BY id", ns, table);
            let mut subscription = client.subscribe(&sql).await.expect("Failed to subscribe");

            // Insert 10 rows
            for i in 0..10 {
                let resp = server
                    .execute_sql(
                        &format!(
                            "INSERT INTO {}.{} (id, content, priority, created_at) VALUES ('msg{}', 'Content {}', {}, {})",
                            ns, table, i, i, i % 3, 1000 + i
                        )
                    )
                    .await?;
                assert_eq!(resp.status, ResponseStatus::Success);
            }

            // Verify notifications
            let mut inserts_received = 0;
            // Await notifications with timeout
            let timeout = tokio::time::sleep(Duration::from_secs(10));
            tokio::pin!(timeout);

            loop {
                tokio::select! {
                    event = subscription.next() => {
                        match event {
                            Some(Ok(ChangeEvent::Insert { rows, .. })) => {
                                inserts_received += 1;
                                // rows is Vec<JsonValue>
                                // We expect 1 row per insert statement as we did single row inserts
                                // Wait, does the notification batch rows? 
                                // Insert { rows: Vec<JsonValue> }
                                // It might contain multiple rows if batched, but we insert one by one.
                                // Just counting "Insert" events might be enough if 1 event = 1 insert query.
                                // Or we should count rows len.
                                // Note: Server sends 1 notification per transaction commit usually.
                                // We are doing separate execute_sql calls.
                            }
                            Some(Ok(ChangeEvent::Ack { .. })) => {
                                // Ignore Ack
                            }
                            Some(Ok(ChangeEvent::InitialDataBatch { .. })) => {
                                // Ignore initial data (table was empty)
                            }
                            Some(Ok(other)) => {
                                println!("Received other event: {:?}", other);
                            }
                            Some(Err(e)) => {
                                panic!("Subscription error: {:?}", e);
                            }
                            None => {
                                panic!("Subscription stream ended unexpectedly");
                            }
                        }

                        if inserts_received >= 10 {
                            break;
                        }
                    }
                    _ = &mut timeout => {
                        panic!("Timed out waiting for 10 insert notifications. Got: {}", inserts_received);
                    }
                }
            }
            
            assert_eq!(inserts_received, 10, "Expected 10 INSERT notifications");

            Ok(())
        })
    })
    .await
    .unwrap();
}
