use super::common::{create_shared_foreign_table, delete_all, unique_name, TestEnv};

#[tokio::test]
async fn e2e_cross_verify_fdw_to_rest() {
    let env = TestEnv::global().await;
    let pg = env.pg_connect().await;
    let table = unique_name("items");
    let qualified_table = format!("e2e.{table}");

    create_shared_foreign_table(&pg, &table, "id TEXT, title TEXT, value INTEGER").await;

    pg.batch_execute(&format!(
        "INSERT INTO {qualified_table} (id, title, value) VALUES ('xv-rest', 'CrossVerify', 999);"
    ))
    .await
    .expect("cross-verify insert");

    let result = env
        .kalamdb_sql(&format!(
            "SELECT id, title, value FROM {qualified_table} WHERE id = 'xv-rest'"
        ))
        .await;
    let result_text = serde_json::to_string(&result).unwrap_or_default();
    assert!(
        result_text.contains("xv-rest"),
        "FDW-inserted row should be visible via REST API: {result_text}"
    );

    pg.execute(&format!("DELETE FROM {qualified_table} WHERE id = $1"), &[&"xv-rest"])
        .await
        .expect("cleanup cross-verify row");
}

#[tokio::test]
#[ntest::timeout(7000)]
async fn e2e_dml_changes_are_visible_in_kalamdb() {
    let env = TestEnv::global().await;
    let pg = env.pg_connect().await;
    let table = unique_name("direct_sync");

    create_shared_foreign_table(&pg, &table, "id TEXT, title TEXT, value INTEGER").await;

    pg.execute(
        &format!("INSERT INTO e2e.{table} (id, title, value) VALUES ($1, $2, $3)"),
        &[&"sync-1", &"Created in PG", &10_i32],
    )
    .await
    .expect("insert should succeed");

    let inserted = env
        .kalamdb_sql(&format!("SELECT id, title, value FROM e2e.{table} WHERE id = 'sync-1'"))
        .await;
    let inserted_text = serde_json::to_string(&inserted).unwrap_or_default();
    assert!(
        inserted_text.contains("sync-1") && inserted_text.contains("Created in PG"),
        "insert should be visible in KalamDB: {inserted_text}"
    );

    pg.execute(
        &format!("UPDATE e2e.{table} SET title = $1, value = $2 WHERE id = $3"),
        &[&"Updated in PG", &99_i32, &"sync-1"],
    )
    .await
    .expect("update should succeed");

    let updated = env
        .kalamdb_sql(&format!("SELECT id, title, value FROM e2e.{table} WHERE id = 'sync-1'"))
        .await;
    let updated_text = serde_json::to_string(&updated).unwrap_or_default();
    assert!(
        updated_text.contains("Updated in PG") && updated_text.contains("99"),
        "update should be visible in KalamDB: {updated_text}"
    );

    pg.execute(&format!("DELETE FROM e2e.{table} WHERE id = $1"), &[&"sync-1"])
        .await
        .expect("delete should succeed");

    let deleted = env
        .kalamdb_sql(&format!("SELECT id FROM e2e.{table} WHERE id = 'sync-1'"))
        .await;
    let deleted_text = serde_json::to_string(&deleted).unwrap_or_default();
    assert!(
        !deleted_text.contains("sync-1"),
        "deleted row should no longer be visible in KalamDB: {deleted_text}"
    );
}

#[tokio::test]
#[ntest::timeout(7000)]
async fn e2e_select_filters_and_postgres_join_work() {
    let env = TestEnv::global().await;
    let pg = env.pg_connect().await;
    let table = unique_name("join_items");

    create_shared_foreign_table(&pg, &table, "id TEXT, title TEXT, value INTEGER").await;
    delete_all(&pg, &format!("e2e.{table}"), "id").await;

    pg.batch_execute(&format!(
        "INSERT INTO e2e.{table} (id, title, value) VALUES \
         ('j1', 'Alpha', 10), \
         ('j2', 'Beta', 20), \
         ('j3', 'Gamma', 30);"
    ))
    .await
    .expect("insert join test rows");

    pg.batch_execute(
        "CREATE TEMP TABLE local_meta (
            id TEXT PRIMARY KEY,
            segment TEXT NOT NULL
        );",
    )
    .await
    .expect("create temp table");

    pg.batch_execute(
        "INSERT INTO local_meta (id, segment) VALUES
         ('j1', 'bronze'),
         ('j2', 'silver'),
         ('j3', 'gold');",
    )
    .await
    .expect("insert local metadata");

    let rows = pg
        .query(
            &format!(
                "SELECT f.id, f.title, m.segment
                 FROM e2e.{table} AS f
                 JOIN local_meta AS m ON m.id = f.id
                 WHERE f.value >= 20
                 ORDER BY f.id"
            ),
            &[],
        )
        .await
        .expect("filter + join query");

    assert_eq!(rows.len(), 2, "expected two joined rows after filter");
    let first: (&str, &str, &str) = (rows[0].get(0), rows[0].get(1), rows[0].get(2));
    let second: (&str, &str, &str) = (rows[1].get(0), rows[1].get(1), rows[1].get(2));
    assert_eq!(first, ("j2", "Beta", "silver"));
    assert_eq!(second, ("j3", "Gamma", "gold"));
}

#[tokio::test]
#[ntest::timeout(7000)]
async fn e2e_search_path_schema_mirror_works_without_namespace_option() {
    let env = TestEnv::global().await;
    let pg = env.pg_connect().await;
    let schema = unique_name("search_ns");
    let table = unique_name("search_items");

    pg.batch_execute(&format!(
        "CREATE SCHEMA IF NOT EXISTS {schema};
         SET search_path TO {schema};
         CREATE FOREIGN TABLE {table} (
             id TEXT,
             title TEXT,
             value INTEGER
         ) SERVER kalam_server
         OPTIONS (table_type 'shared');"
    ))
    .await
    .expect("create mirrored foreign table via search_path");

    pg.execute(
        &format!("INSERT INTO {schema}.{table} (id, title, value) VALUES ($1, $2, $3)"),
        &[&"spath-1", &"From search_path", &7_i32],
    )
    .await
    .expect("insert through schema-mirrored foreign table");

    let rows = pg
        .query(
            &format!("SELECT id, title, value FROM {schema}.{table} WHERE id = $1"),
            &[&"spath-1"],
        )
        .await
        .expect("select through schema-mirrored foreign table");
    assert_eq!(rows.len(), 1, "expected one mirrored row through PostgreSQL");

    let result = env
        .kalamdb_sql(&format!("SELECT id, title, value FROM {schema}.{table} WHERE id = 'spath-1'"))
        .await;
    let result_text = serde_json::to_string(&result).unwrap_or_default();
    assert!(
        result_text.contains("spath-1") && result_text.contains("From search_path"),
        "search_path-mirrored foreign table should write into KalamDB namespace {schema}: {result_text}"
    );

    pg.batch_execute(&format!(
        "RESET search_path;
         DROP FOREIGN TABLE IF EXISTS {schema}.{table};
         DROP SCHEMA IF EXISTS {schema} CASCADE;"
    ))
    .await
    .ok();
}
