pub use crate::e2e_ddl_common::DdlTestEnv;

pub async fn ensure_schema_exists(pg: &tokio_postgres::Client, schema: &str) {
    pg.batch_execute(&format!("CREATE SCHEMA IF NOT EXISTS {schema};"))
        .await
        .expect("CREATE SCHEMA");
}

pub async fn pg_kalam_exec(pg: &tokio_postgres::Client, sql: &str) -> String {
    let row = pg.query_one("SELECT kalam_exec($1)", &[&sql]).await.expect("SELECT kalam_exec");
    row.get(0)
}

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
