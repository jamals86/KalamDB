use crate::common::fixtures;
use crate::common::flush_helpers;
use crate::common::TestServer;
use kalamdb_api::models::ResponseStatus;
use tokio::time::{sleep, Duration};

async fn create_table(server: &TestServer, ns: &str, table: &str) {
    fixtures::create_namespace(server, ns).await;
    let sql = format!(
        r#"CREATE TABLE {ns}.{table} (
            id INT PRIMARY KEY,
            data TEXT
        ) WITH (
            TYPE = 'USER',
            STORAGE_ID = 'local',
            FLUSH_POLICY = 'rows:8'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_queries_remain_consistent_during_flush() {
    let server = TestServer::new().await;
    let ns = "flush_consistency_ns";
    let table = "items";
    create_table(&server, ns, table).await;

    for i in 1..=25 {
        let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'v{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    let flusher = {
        let server = server.clone();
        let ns = ns.to_string();
        let table = table.to_string();
        tokio::spawn(async move {
            for _ in 0..3 {
                flush_helpers::execute_flush_synchronously(&server, &ns, &table)
                    .await
                    .expect("flush failed");
                sleep(Duration::from_millis(30)).await;
            }
        })
    };

    let reader = {
        let server = server.clone();
        let ns = ns.to_string();
        let table = table.to_string();
        tokio::spawn(async move {
            for _ in 0..5 {
                let resp = server
                    .execute_sql_as_user(
                        &format!("SELECT COUNT(*) as cnt FROM {ns}.{table}"),
                        "user_a",
                    )
                    .await;
                assert_eq!(resp.status, ResponseStatus::Success);
                let cnt = resp.results[0].rows.as_ref().unwrap()[0]
                    .get("cnt")
                    .unwrap()
                    .as_i64()
                    .unwrap();
                assert_eq!(cnt, 25);
                sleep(Duration::from_millis(10)).await;
            }
        })
    };

    let _ = tokio::join!(flusher, reader);
}
