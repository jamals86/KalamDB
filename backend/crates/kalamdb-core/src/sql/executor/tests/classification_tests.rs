//! Tests for SQL statement classification using kalamdb_sql::SqlStatement
//!
//! These tests verify that SqlStatement::classify() correctly identifies
//! different SQL statement types for proper routing to handlers.

use kalamdb_sql::statement_classifier::SqlStatement;

#[test]
fn test_classify_select() {
    let sql = "SELECT * FROM users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Select));

    let sql = "select id, name from products where price > 10";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Select));
}

#[test]
fn test_classify_insert() {
    let sql = "INSERT INTO users (id, name) VALUES (1, 'Alice')";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Insert));

    let sql = "insert into products values (1, 'Widget', 9.99)";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Insert));
}

#[test]
fn test_classify_update() {
    let sql = "UPDATE users SET name = 'Bob' WHERE id = 1";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Update));

    let sql = "update products set price = 19.99 where id = 1";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Update));
}

#[test]
fn test_classify_delete() {
    let sql = "DELETE FROM users WHERE id = 1";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Delete));

    let sql = "delete from products where price < 5";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Delete));
}

#[test]
fn test_classify_create_table() {
    let sql = "CREATE TABLE users (id INTEGER, name TEXT)";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CreateTable));

    let sql = "create table products (id int, price decimal)";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CreateTable));
}

#[test]
fn test_classify_alter_table() {
    let sql = "ALTER TABLE users ADD COLUMN email TEXT";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::AlterTable));

    let sql = "alter table products drop column description";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::AlterTable));
}

#[test]
fn test_classify_drop_table() {
    let sql = "DROP TABLE users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::DropTable));

    let sql = "drop table if exists products";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::DropTable));
}

#[test]
fn test_classify_transactions() {
    let sql = "BEGIN";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::BeginTransaction));

    let sql = "COMMIT";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CommitTransaction));

    let sql = "ROLLBACK";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::RollbackTransaction));

    // Case insensitive
    let sql = "begin transaction";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::BeginTransaction));
}

#[test]
fn test_classify_create_namespace() {
    let sql = "CREATE NAMESPACE analytics";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CreateNamespace));

    let sql = "create namespace if not exists reporting";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CreateNamespace));
}

#[test]
fn test_classify_show_tables() {
    let sql = "SHOW TABLES";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::ShowTables));

    let sql = "show tables from analytics";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::ShowTables));
}

#[test]
fn test_classify_describe_table() {
    let sql = "DESCRIBE users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::DescribeTable));

    let sql = "desc analytics.events";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::DescribeTable));
}

#[test]
fn test_classify_flush_table() {
    let sql = "FLUSH TABLE users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::FlushTable));

    let sql = "flush table analytics.events";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::FlushTable));
}

#[test]
fn test_classify_create_user() {
    let sql = "CREATE USER alice WITH PASSWORD 'secret123'";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CreateUser));

    let sql = "create user bob with role dba";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::CreateUser));
}

#[test]
fn test_classify_unknown() {
    let sql = "VACUUM ANALYZE";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Unknown));

    let sql = "PRAGMA table_info(users)";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Unknown));

    let sql = "EXPLAIN SELECT * FROM users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Unknown));
}

#[test]
fn test_classify_case_insensitive() {
    // All variants should work regardless of case
    assert!(matches!(SqlStatement::classify("SELECT * FROM users"), SqlStatement::Select));
    assert!(matches!(SqlStatement::classify("select * from users"), SqlStatement::Select));
    assert!(matches!(SqlStatement::classify("SeLeCt * FrOm users"), SqlStatement::Select));

    assert!(matches!(SqlStatement::classify("CREATE TABLE t (id INT)"), SqlStatement::CreateTable));
    assert!(matches!(SqlStatement::classify("create table t (id int)"), SqlStatement::CreateTable));
}

#[test]
fn test_classify_with_leading_whitespace() {
    let sql = "  \n  SELECT * FROM users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Select));

    let sql = "\t\tINSERT INTO users VALUES (1, 'Alice')";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Insert));
}

#[test]
fn test_classify_with_comments() {
    // Line comments
    let sql = "-- This is a comment\nSELECT * FROM users";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Select));

    // Block comments
    let sql = "/* Multi-line\n   comment */\nINSERT INTO users VALUES (1, 'Alice')";
    assert!(matches!(SqlStatement::classify(sql), SqlStatement::Insert));
}
