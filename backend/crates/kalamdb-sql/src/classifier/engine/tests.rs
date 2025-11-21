use super::*;
use crate::classifier::types::{SqlStatement, SqlStatementKind};
use kalamdb_commons::models::NamespaceId;
use kalamdb_commons::Role;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_classify_select() {
        let sql = "SELECT * FROM users";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Select));
    }

    #[test]
    fn test_classify_insert() {
        let sql = "INSERT INTO users (id, name) VALUES (1, 'Alice')";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Insert(_)));
    }

    #[test]
    fn test_classify_create_table() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR)";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_create_user_table() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR) WITH (TYPE='USER')";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_create_user_table_with_flush_policy() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR) WITH (TYPE='USER', FLUSH_POLICY='rows:100')";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_create_shared_table() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR) WITH (TYPE='SHARED')";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_create_shared_table_with_access_level() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR) WITH (TYPE='SHARED', ACCESS_LEVEL='public')";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_create_stream_table() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR) WITH (TYPE='STREAM', TTL_SECONDS=60)";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_create_stream_table_with_retention() {
        let sql = "CREATE TABLE users (id INT, name VARCHAR) WITH (TYPE='STREAM', TTL_SECONDS=60, MAX_BUFFERED_ROWS=1000)";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateTable(_)));
    }

    #[test]
    fn test_classify_drop_table() {
        let sql = "DROP TABLE users";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::DropTable(_)));
    }

    #[test]
    fn test_classify_show_tables() {
        let sql = "SHOW TABLES";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::ShowTables(_)));
    }

    #[test]
    fn test_classify_describe_table() {
        let sql = "DESCRIBE TABLE users";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::DescribeTable(_)));
    }

    #[test]
    fn test_classify_create_namespace() {
        let sql = "CREATE NAMESPACE my_ns";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateNamespace(_)));
    }

    #[test]
    fn test_classify_create_storage() {
        let sql = "CREATE STORAGE s3_storage TYPE 's3' NAME 'S3 Storage' BASE_DIRECTORY 's3://bucket/' SHARED_TABLES_TEMPLATE '{ns}/{table}' USER_TABLES_TEMPLATE '{ns}/{table}/{user}'";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateStorage(_)));
    }

    #[test]
    fn test_classify_create_view() {
        let sql = "CREATE VIEW default.simple_view AS SELECT 1";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateView(_)));
    }

    #[test]
    fn test_classify_unknown() {
        let sql = "FOOBAR BAZ";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Unknown));
    }

    #[test]
    fn test_classify_empty() {
        let sql = "";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Unknown));
    }

    #[test]
    fn test_classify_whitespace() {
        let sql = "   ";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Unknown));
    }

    #[test]
    fn test_classify_case_insensitive() {
        let sql = "select * from users";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Select));
    }

    #[test]
    fn test_classify_with_comments() {
        let sql = "-- This is a comment\nSELECT * FROM users";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Select));
    }

    #[test]
    fn test_classify_multiline() {
        let sql = "SELECT *\nFROM users\nWHERE id = 1";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Select));
    }

    #[test]
    fn test_classify_create_user() {
        let sql = "CREATE USER alice WITH PASSWORD 'password123' ROLE user";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::CreateUser(_)));
    }

    #[test]
    fn test_classify_alter_user() {
        let sql = "ALTER USER alice SET PASSWORD 'newpassword'";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::AlterUser(_)));
    }

    #[test]
    fn test_classify_drop_user() {
        let sql = "DROP USER 'alice'";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::DropUser(_)));
    }

    #[test]
    fn test_classify_kill_job() {
        let sql = "KILL JOB 'job-123'";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::KillJob(_)));
    }

    #[test]
    fn test_classify_kill_live_query() {
        let sql = "KILL LIVE QUERY 'user-conn-table-query'";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::KillLiveQuery(_)));
    }

    #[test]
    fn test_classify_subscribe() {
        let sql = "SUBSCRIBE TO SELECT * FROM default.users";
        let stmt = SqlStatement::classify(sql);
        assert!(matches!(stmt.kind, SqlStatementKind::Subscribe(_)));
    }

    #[test]
    fn test_classify_transactions() {
        assert!(matches!(
            SqlStatement::classify("BEGIN").kind,
            SqlStatementKind::BeginTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("COMMIT").kind,
            SqlStatementKind::CommitTransaction
        ));
        assert!(matches!(
            SqlStatement::classify("ROLLBACK").kind,
            SqlStatementKind::RollbackTransaction
        ));
    }

    #[test]
    fn test_authorization_check() {
        let default_ns = NamespaceId::new("default");

        // Admin user (System) - can do anything
        let stmt =
            SqlStatement::classify_and_parse("CREATE NAMESPACE test", &default_ns, Role::System)
                .unwrap();
        assert!(matches!(stmt.kind, SqlStatementKind::CreateNamespace(_)));

        // Regular user - cannot create namespace
        let err =
            SqlStatement::classify_and_parse("CREATE NAMESPACE test", &default_ns, Role::User)
                .unwrap_err();
        match err {
            crate::classifier::types::StatementClassificationError::Unauthorized(msg) => {
                assert!(msg.contains("Admin privileges"));
            }
            _ => panic!("Expected Unauthorized error"),
        }

        // Regular user - can select
        let stmt = SqlStatement::classify_and_parse("SELECT * FROM users", &default_ns, Role::User)
            .unwrap();
        assert!(matches!(stmt.kind, SqlStatementKind::Select));

        // Regular user - can show tables
        let stmt =
            SqlStatement::classify_and_parse("SHOW TABLES", &default_ns, Role::User).unwrap();
        assert!(matches!(stmt.kind, SqlStatementKind::ShowTables(_)));
    }

    #[test]
    fn test_extract_as_user() {
        // Test INSERT with AS USER
        let sql = "INSERT INTO users (id) VALUES (1) AS USER 'user_123'";
        let (cleaned, user_id) = SqlStatement::extract_as_user(sql);
        assert_eq!(cleaned, "INSERT INTO users (id) VALUES (1)");
        assert_eq!(user_id.unwrap().as_str(), "user_123");

        // Test DELETE with AS USER
        let sql = "DELETE FROM users WHERE id=1 AS USER \"user_456\"";
        let (cleaned, user_id) = SqlStatement::extract_as_user(sql);
        assert_eq!(cleaned, "DELETE FROM users WHERE id=1");
        assert_eq!(user_id.unwrap().as_str(), "user_456");

        // Test UPDATE with AS USER
        let sql = "UPDATE users SET name='Alice' WHERE id=1 AS USER 'user_789'";
        let (cleaned, user_id) = SqlStatement::extract_as_user(sql);
        assert_eq!(cleaned, "UPDATE users SET name='Alice' WHERE id=1");
        assert_eq!(user_id.unwrap().as_str(), "user_789");

        // Test without AS USER
        let sql = "SELECT * FROM users";
        let (cleaned, user_id) = SqlStatement::extract_as_user(sql);
        assert_eq!(cleaned, "SELECT * FROM users");
        assert!(user_id.is_none());

        // Test with AS USER in middle (should still work but might be weird SQL)
        let sql = "INSERT INTO users AS USER 'user_123' (id) VALUES (1)";
        let (cleaned, user_id) = SqlStatement::extract_as_user(sql);
        assert_eq!(cleaned, "INSERT INTO users (id) VALUES (1)");
        assert_eq!(user_id.unwrap().as_str(), "user_123");
    }
}
