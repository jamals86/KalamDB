pub use crate::e2e_common::*;

pub fn unique_name(prefix: &str) -> String {
    use std::sync::atomic::{AtomicU64, Ordering};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, Ordering::Relaxed);
    let ts = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_millis();
    format!("{prefix}_{ts}_{n}")
}

pub async fn ensure_schema_exists(client: &tokio_postgres::Client, schema: &str) {
    client
        .batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {schema};"))
        .await
        .expect("create scenario schema");
}

pub async fn create_shared_kalam_table_in_schema(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
    columns: &str,
) {
    ensure_schema_exists(client, schema).await;
    client
        .batch_execute(&format!("DROP FOREIGN TABLE IF EXISTS {schema}.{table};"))
        .await
        .ok();
    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.{table} ({columns}) USING kalamdb WITH (type = 'shared');"
        ))
        .await
        .expect("create shared Kalam table");
}

pub async fn create_user_kalam_table_in_schema(
    client: &tokio_postgres::Client,
    schema: &str,
    table: &str,
    columns: &str,
) {
    ensure_schema_exists(client, schema).await;
    client
        .batch_execute(&format!("DROP FOREIGN TABLE IF EXISTS {schema}.{table};"))
        .await
        .ok();
    client
        .batch_execute(&format!(
            "CREATE TABLE {schema}.{table} ({columns}) USING kalamdb WITH (type = 'user');"
        ))
        .await
        .expect("create user Kalam table");
}

pub async fn drop_kalam_tables(client: &tokio_postgres::Client, schema: &str, tables: &[String]) {
    for table in tables {
        client
            .batch_execute(&format!("DROP FOREIGN TABLE IF EXISTS {schema}.{table};"))
            .await
            .ok();
    }
    client
        .batch_execute(&format!("DROP SCHEMA IF EXISTS {schema} CASCADE;"))
        .await
        .ok();
}
