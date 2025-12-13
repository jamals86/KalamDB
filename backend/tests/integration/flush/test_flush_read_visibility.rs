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
            FLUSH_POLICY = 'rows:20'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_flush_preserves_read_visibility() {
    let server = TestServer::new().await;
    let ns = "flush_vis_ns";
    let table = "items";
    create_table(&server, ns, table).await;

    for i in 1..=30 {
        let insert_sql = format!(
            "INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'val_{i}')"
        );
        let resp = server.execute_sql_as_user(&insert_sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success, "insert {i} failed: {:?}", resp.error);
    }

    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("flush 1 failed");

    for i in 31..=35 {
        let insert_sql = format!(
            "INSERT INTO {ns}.{table} (id, data) VALUES ({i}, 'val_{i}')"
        );
        let resp = server.execute_sql_as_user(&insert_sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success, "insert {i} failed: {:?}", resp.error);
    }

    flush_helpers::execute_flush_synchronously(&server, ns, table)
        .await
        .expect("flush 2 failed");

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
    assert_eq!(cnt, 35, "expected all rows visible after multiple flushes");
}
