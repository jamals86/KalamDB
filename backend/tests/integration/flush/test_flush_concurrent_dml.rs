use crate::common::fixtures;
use crate::common::flush_helpers;
use crate::common::TestServer;
use kalamdb_api::models::ResponseStatus;
use tokio::time::{sleep, Duration};

/// Emulates concurrent flushing while inserts/updates/deletes occur.
/// Ensures data remains consistent across hot and cold storage.
#[actix_web::test]
async fn test_flush_concurrent_dml() {
    let server = TestServer::new().await;
    let ns = "flush_concurrent_ns";
    let table = "items";
    fixtures::create_namespace(&server, ns).await;

    let create_sql = format!(
        r#"CREATE TABLE {}.{} (
            id INT PRIMARY KEY,
            data TEXT
        ) WITH (
            TYPE = 'USER',
            STORAGE_ID = 'local',
            FLUSH_POLICY = 'rows:50'
        )"#,
        ns, table
    );
    let create_resp = server.execute_sql_as_user(&create_sql, "system").await;
    assert_eq!(create_resp.status, ResponseStatus::Success, "create failed: {:?}", create_resp.error);

    let writer = {
        let server = server.clone();
        let ns = ns.to_string();
        let table = table.to_string();
        tokio::spawn(async move {
            for i in 1..=120 {
                let insert_sql = format!(
                    "INSERT INTO {}.{} (id, data) VALUES ({}, 'value_{}')",
                    ns, table, i, i
                );
                let resp = server.execute_sql_as_user(&insert_sql, "user_a").await;
                assert_eq!(resp.status, ResponseStatus::Success, "insert {} failed: {:?}", i, resp.error);

                if i % 20 == 0 {
                    let update_sql = format!(
                        "UPDATE {}.{} SET data = 'updated_{}' WHERE id = {}",
                        ns, table, i, i
                    );
                    let resp = server.execute_sql_as_user(&update_sql, "user_a").await;
                    assert_eq!(resp.status, ResponseStatus::Success, "update {} failed: {:?}", i, resp.error);
                }

                if i % 30 == 0 {
                    let delete_id = i - 10;
                    let delete_sql = format!(
                        "DELETE FROM {}.{} WHERE id = {}",
                        ns, table, delete_id
                    );
                    let resp = server.execute_sql_as_user(&delete_sql, "user_a").await;
                    assert_eq!(resp.status, ResponseStatus::Success, "delete {} failed: {:?}", delete_id, resp.error);
                }
            }
        })
    };

    let flusher = {
        let server = server.clone();
        let ns = ns.to_string();
        let table = table.to_string();
        tokio::spawn(async move {
            for _ in 0..5 {
                let _ = flush_helpers::execute_flush_synchronously(&server, &ns, &table)
                    .await
                    .expect("flush failed");
                sleep(Duration::from_millis(40)).await;
            }
        })
    };

    let _ = tokio::join!(writer, flusher);

    // Final flush to ensure cold storage is up to date.
    let _ = flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("final flush failed");

    // Validate row count (4 deletes of distinct ids: 20, 50, 80, 110).
    let count_resp = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as cnt FROM {}.{}", ns, table),
            "user_a",
        )
        .await;
    assert_eq!(count_resp.status, ResponseStatus::Success);
    let cnt = count_resp.results[0]
        .rows
        .as_ref()
        .unwrap()[0]
        .get("cnt")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(cnt, 116, "expected 116 rows after deletes");

    // Confirm deleted ids are gone.
    for deleted_id in [20, 50, 80, 110] {
        let sel = server
            .execute_sql_as_user(
                &format!("SELECT id FROM {}.{} WHERE id = {}", ns, table, deleted_id),
                "user_a",
            )
            .await;
        assert_eq!(sel.status, ResponseStatus::Success);
        assert_eq!(sel.results[0].rows.as_ref().map(|r| r.len()).unwrap_or(0), 0);
    }

    // Confirm updates persisted through flushes.
    for updated_id in [40, 60, 100, 120] {
        let sel = server
            .execute_sql_as_user(
                &format!("SELECT data FROM {}.{} WHERE id = {}", ns, table, updated_id),
                "user_a",
            )
            .await;
        assert_eq!(sel.status, ResponseStatus::Success);
        let rows = sel.results[0].rows.as_ref().unwrap();
        assert_eq!(rows.len(), 1);
        let val = rows[0].get("data").unwrap().as_str().unwrap();
        assert_eq!(val, format!("updated_{}", updated_id));
    }
}
