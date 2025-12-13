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
            FLUSH_POLICY = 'rows:10'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_updates_persist_across_flushes() {
    let server = TestServer::new().await;
    let ns = "flush_update_ns";
    let table = "items";
    create_table(&server, ns, table).await;

    let insert = format!("INSERT INTO {ns}.{table} (id, data) VALUES (1, 'old')");
    let resp = server.execute_sql_as_user(&insert, "user_a").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("initial flush failed");

    let update = format!("UPDATE {ns}.{table} SET data = 'new' WHERE id = 1");
    let resp = server.execute_sql_as_user(&update, "user_a").await;
    assert_eq!(resp.status, ResponseStatus::Success);

    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("second flush failed");

    let sel = server
        .execute_sql_as_user(
            &format!("SELECT data FROM {ns}.{table} WHERE id = 1"),
            "user_a",
        )
        .await;
    assert_eq!(sel.status, ResponseStatus::Success);
    let rows = sel.results[0].rows.as_ref().unwrap();
    assert_eq!(rows.len(), 1);
    let val = rows[0].get("data").unwrap().as_str().unwrap();
    assert_eq!(val, "new");
}
