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
            FLUSH_POLICY = 'rows:5'
        )"#
    );
    let resp = server.execute_sql_as_user(&sql, "system").await;
    assert_eq!(resp.status, ResponseStatus::Success, "create failed: {:?}", resp.error);
}

#[actix_web::test]
async fn test_flush_isolation_between_tables() {
    let server = TestServer::new().await;
    let ns = "flush_iso_ns";
    let t1 = "alpha";
    let t2 = "beta";
    create_table(&server, ns, t1).await;
    create_table(&server, ns, t2).await;

    for i in 1..=6 {
        let sql = format!("INSERT INTO {ns}.{t1} (id, data) VALUES ({i}, 'a{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }
    for i in 1..=4 {
        let sql = format!("INSERT INTO {ns}.{t2} (id, data) VALUES ({i}, 'b{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    // Flush only first table
    flush_helpers::execute_flush_synchronously(&server, ns, t1)
        .await
        .expect("flush t1 failed");

    // Insert more into second table after first flush
    for i in 5..=7 {
        let sql = format!("INSERT INTO {ns}.{t2} (id, data) VALUES ({i}, 'b{i}')");
        let resp = server.execute_sql_as_user(&sql, "user_a").await;
        assert_eq!(resp.status, ResponseStatus::Success);
    }

    flush_helpers::execute_flush_synchronously(&server, ns, t2)
        .await
        .expect("flush t2 failed");

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
    assert_eq!(cnt1, 6);

    let count_t2 = server
        .execute_sql_as_user(
            &format!("SELECT COUNT(*) as cnt FROM {ns}.{t2}"),
            "user_a",
        )
        .await;
    assert_eq!(count_t2.status, ResponseStatus::Success);
    let cnt2 = count_t2.results[0].rows.as_ref().unwrap()[0]
        .get("cnt")
        .unwrap()
        .as_i64()
        .unwrap();
    assert_eq!(cnt2, 7);
}
