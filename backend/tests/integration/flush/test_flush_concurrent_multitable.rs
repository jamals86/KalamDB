use crate::common::fixtures;
use crate::common::flush_helpers;
use crate::common::TestServer;
use kalamdb_api::models::ResponseStatus;
use tokio::time::{sleep, Duration};

async fn create_table(server: &TestServer, ns: &str, table: &str, flush_policy: &str) {
    fixtures::create_namespace(server, ns).await;
    let sql = format!(
        r#"CREATE TABLE {ns}.{table} (
            id INT PRIMARY KEY,
            data TEXT
        ) WITH (
            TYPE = 'USER',
            STORAGE_ID = 'local',
            FLUSH_POLICY = '{flush_policy}'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_concurrent_flushes_across_tables() {
    let server = TestServer::new().await;
    let ns = "flush_concurrent_multi_ns";
    let t1 = "alpha";
    let t2 = "beta";
    create_table(&server, ns, t1, "rows:30").await;
    create_table(&server, ns, t2, "rows:30").await;

    let writer1 = {
        let server = server.clone();
        let ns = ns.to_string();
        let table = t1.to_string();
        tokio::spawn(async move {
            for i in 1..=120 {
                let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'a{i}')");
                let resp = server.execute_sql_as_user(&sql, "user_a").await;
                assert_eq!(resp.status, ResponseStatus::Success, "t1 insert {i} failed: {:?}", resp.error);
                if i % 25 == 0 {
                    let upd = format!("UPDATE {ns}.{table} SET data = 'upd_{i}' WHERE id = {i}");
                    let resp = server.execute_sql_as_user(&upd, "user_a").await;
                    assert_eq!(resp.status, ResponseStatus::Success, "t1 update {i} failed: {:?}", resp.error);
                }
            }
        })
    };

    let writer2 = {
        let server = server.clone();
        let ns = ns.to_string();
        let table = t2.to_string();
        tokio::spawn(async move {
            for i in 1..=120 {
                let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'b{i}')");
                let resp = server.execute_sql_as_user(&sql, "user_b").await;
                assert_eq!(resp.status, ResponseStatus::Success, "t2 insert {i} failed: {:?}", resp.error);
                if i % 40 == 0 {
                    let del = format!("DELETE FROM {ns}.{table} WHERE id = {}", i - 10);
                    let resp = server.execute_sql_as_user(&del, "user_b").await;
                    assert_eq!(resp.status, ResponseStatus::Success, "t2 delete failed: {:?}", resp.error);
                }
            }
        })
    };

    let flusher = {
        let server = server.clone();
        let ns = ns.to_string();
        tokio::spawn(async move {
            for _ in 0..6 {
                flush_helpers::execute_flush_synchronously(&server, &ns, t1)
                    .await
                    .expect("flush t1 failed");
                flush_helpers::execute_flush_synchronously(&server, &ns, t2)
                    .await
                    .expect("flush t2 failed");
                sleep(Duration::from_millis(30)).await;
            }
        })
    };

    let _ = tokio::join!(writer1, writer2, flusher);

    // Final flush to capture latest versions
    flush_helpers::execute_flush_synchronously(&server, ns, t1)
        .await
        .expect("final flush t1 failed");
    flush_helpers::execute_flush_synchronously(&server, ns, t2)
        .await
        .expect("final flush t2 failed");

    // Validate counts
    let count_t1 = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as cnt FROM {ns}.{t1}"),
            "user_a",
        )
        .await;
    assert_eq!(count_t1.status, ResponseStatus::Success);
    let cnt1 = count_t1.results[0].rows.as_ref().unwrap()[0]
        .get("cnt")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(cnt1, 120, "t1 expected all rows present (no deletes)");

    let count_t2 = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as cnt FROM {ns}.{t2}"),
            "user_b",
        )
        .await;
    assert_eq!(count_t2.status, ResponseStatus::Success);
    let cnt2 = count_t2.results[0].rows.as_ref().unwrap()[0]
        .get("cnt")
        .unwrap()
        .as_i64()
        .unwrap();
    // Deletes at i=40,80,120 remove ids 30,70,110 -> 3 deletions
    assert_eq!(cnt2, 117, "t2 count should reflect deletes");

    // Spot check update in t1
    let sel = server
        .execute_sql_as_user(
            &format!("SELECT data FROM {ns}.{t1} WHERE id = 100"),
            "user_a",
        )
        .await;
    assert_eq!(sel.status, ResponseStatus::Success);
    let val = sel.results[0].rows.as_ref().unwrap()[0]
        .get("data")
        .unwrap()
        .as_str()
        .unwrap();
    assert_eq!(val, "upd_100");

    // Spot check delete in t2
    let sel = server
        .execute_sql_as_user(
            &format!("SELECT id FROM {ns}.{t2} WHERE id = 30"),
            "user_b",
        )
        .await;
    assert_eq!(sel.status, ResponseStatus::Success);
    assert_eq!(sel.results[0].rows.as_ref().map(|r| r.len()).unwrap_or(0), 0);
}
