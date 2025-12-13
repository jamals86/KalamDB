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
            FLUSH_POLICY = 'rows:15'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_deletes_persist_across_flushes() {
    let server = TestServer::new().await;
    let ns = "flush_delete_ns";
    let table = "items";
    create_table(&server, ns, table).await;

    for i in 1..=5 {
        let sql = format!("INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'v{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("initial flush failed");

    let delete_sql = format!("DELETE FROM {ns}.{table} WHERE id = 3");
    let resp = server.execute_sql_as_user(&delete_sql, "user_a").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("second flush failed");

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
    assert_eq!(cnt, 4, "delete should persist after flush");

    let sel = server
        .execute_sql_as_user(
            &format!("SELECT id FROM {ns}.{table} WHERE id = 3"),
            "user_a",
        )
        .await;
    assert_eq!(sel.status, ResponseStatus::Success);
    assert_eq!(sel.results[0].rows.as_ref().map(|r| r.len()).unwrap_or(0), 0);
}
