use crate::common::fixtures;
use crate::common::flush_helpers;
use crate::common::TestServer;
use kalamdb_api::models::ResponseStatus;

async fn create_table(server: &TestServer, ns: &str, table: &str) {
    fixtures::create_namespace(server, ns).await;
    let sql = format!(
        r#"CREATE TABLE {ns}.{table} (
            id INT PRIMARY KEY,
            data TEXT
        ) WITH (
            TYPE = 'USER',
            STORAGE_ID = 'local',
            FLUSH_POLICY = 'rows:25'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_interleaved_batches_and_flushes() {
    let server = TestServer::new().await;
    let ns = "flush_interleave_ns";
    let table = "items";
    create_table(&server, ns, table).await;

    // Batch 1
    for i in 1..=30 {
        let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'p{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }
    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("flush 1 failed");

    // Batch 2 with updates on some prior rows
    for i in 31..=50 {
        let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'p{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }
    for id in [5, 10, 20] {
        let sql = format!("UPDATE {ns}.{table} SET data = 'upd_{id}' WHERE id = {id}");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }
    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("flush 2 failed");

    // Batch 3 with deletions
    for id in [3, 4, 7] {
        let sql = format!("DELETE FROM {ns}.{table} WHERE id = {id}");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }
    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("flush 3 failed");

    let count = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as cnt FROM {ns}.{table}"),
            "user_a",
        )
        .await;
    assert_eq!(count.status, ResponseStatus::Success);
    let cnt = count.results[0].rows.as_ref().unwrap()[0]
        .get("cnt")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(cnt, 47, "should reflect inserts minus deletions");

    // Validate updates survived across flushes
    for id in [5, 10, 20] {
        let sel = server
            .execute_sql_as_user(
                &format!("SELECT data FROM {ns}.{table} WHERE id = {id}"),
                "user_a",
            )
            .await;
        assert_eq!(sel.status, ResponseStatus::Success);
        let val = sel.results[0].rows.as_ref().unwrap()[0]
            .get("data")
            .unwrap()
            .as_str()
            .unwrap();
        assert_eq!(val, format!("upd_{id}"));
    }

    // Validate deletes persisted
    for id in [3, 4, 7] {
        let sel = server
            .execute_sql_as_user(
                &format!("SELECT id FROM {ns}.{table} WHERE id = {id}"),
                "user_a",
            )
            .await;
        assert_eq!(sel.status, ResponseStatus::Success);
        assert_eq!(sel.results[0].rows.as_ref().map(|r| r.len()).unwrap_or(0), 0);
    }
}
