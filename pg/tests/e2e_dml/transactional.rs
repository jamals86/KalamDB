use super::common::{
    await_user_shard_leader, count_rows, create_user_foreign_table, postgres_error_text,
    set_user_id, unique_name, TestEnv,
};

#[tokio::test]
#[ntest::timeout(7000)]
async fn e2e_transaction_begin_commit_persists_rows() {
    let env = TestEnv::global().await;
    let mut pg = env.pg_connect().await;
    let table = unique_name("profiles_tx_commit");
    let qualified_table = format!("e2e.{table}");

    create_user_foreign_table(
        &pg,
        &table,
        "id TEXT, name TEXT, age INTEGER, _userid TEXT, _seq BIGINT, _deleted BOOLEAN",
    )
    .await;
    set_user_id(&pg, "txn-commit-user").await;
    await_user_shard_leader("txn-commit-user").await;

    let tx = pg.transaction().await.expect("begin");
    tx.execute(
        &format!("INSERT INTO {qualified_table} (id, name, age) VALUES ($1, $2, $3)"),
        &[&"commit-1", &"Alice", &30_i32],
    )
    .await
    .expect("insert first row");
    tx.execute(
        &format!("INSERT INTO {qualified_table} (id, name, age) VALUES ($1, $2, $3)"),
        &[&"commit-2", &"Bob", &31_i32],
    )
    .await
    .expect("insert second row");
    tx.commit().await.expect("commit");

    let count = count_rows(&pg, &qualified_table, None).await;
    assert_eq!(count, 2, "committed transaction should persist both rows");
}

#[tokio::test]
#[ntest::timeout(7000)]
async fn e2e_transaction_begin_rollback_discards_rows() {
    let env = TestEnv::global().await;
    let mut pg = env.pg_connect().await;
    let table = unique_name("profiles_tx_rollback");
    let qualified_table = format!("e2e.{table}");

    create_user_foreign_table(
        &pg,
        &table,
        "id TEXT, name TEXT, age INTEGER, _userid TEXT, _seq BIGINT, _deleted BOOLEAN",
    )
    .await;
    set_user_id(&pg, "txn-rollback-user").await;
    await_user_shard_leader("txn-rollback-user").await;

    let tx = pg.transaction().await.expect("begin");
    tx.execute(
        &format!("INSERT INTO {qualified_table} (id, name, age) VALUES ($1, $2, $3)"),
        &[&"rollback-1", &"Alice", &30_i32],
    )
    .await
    .expect("insert row");
    tx.rollback().await.expect("rollback");

    let count = count_rows(&pg, &qualified_table, None).await;
    assert_eq!(count, 0, "rolled back transaction should not persist rows");
}

#[tokio::test]
#[ntest::timeout(7000)]
async fn e2e_transaction_duplicate_primary_key_commit_fails() {
    let env = TestEnv::global().await;
    let mut pg = env.pg_connect().await;
    let table = unique_name("profiles_tx_duplicate");
    let qualified_table = format!("e2e.{table}");

    create_user_foreign_table(
        &pg,
        &table,
        "id TEXT, name TEXT, age INTEGER, _userid TEXT, _seq BIGINT, _deleted BOOLEAN",
    )
    .await;
    set_user_id(&pg, "txn-duplicate-user").await;
    await_user_shard_leader("txn-duplicate-user").await;

    let tx = pg.transaction().await.expect("begin");
    tx.execute(
        &format!("INSERT INTO {qualified_table} (id, name, age) VALUES ($1, $2, $3)"),
        &[&"dup-1", &"Alice", &30_i32],
    )
    .await
    .expect("first insert should stage");
    tx.execute(
        &format!("INSERT INTO {qualified_table} (id, name, age) VALUES ($1, $2, $3)"),
        &[&"dup-1", &"Alice 2", &31_i32],
    )
    .await
    .expect("duplicate insert should stage until commit");

    let err = tx
        .commit()
        .await
        .expect_err("duplicate primary key insert should fail at commit");

    let message = postgres_error_text(&err);
    assert!(
        message.contains("Primary key violation")
            || message.contains("already exists")
            || message.contains("appears multiple times")
            || message.contains("db error"),
        "unexpected duplicate key error: {message}"
    );

    let reader = env.pg_connect().await;
    set_user_id(&reader, "txn-duplicate-user").await;
    await_user_shard_leader("txn-duplicate-user").await;
    let count = count_rows(&reader, &qualified_table, Some("id = 'dup-1'")) .await;
    assert_eq!(count, 0, "failed commit should leave no committed rows");
}