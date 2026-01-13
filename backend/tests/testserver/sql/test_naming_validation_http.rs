//! Naming validation tests over the real HTTP SQL API.

#[path = "../../common/testserver/mod.rs"]
mod test_support;

use kalam_link::models::ResponseStatus;
use tokio::time::Duration;

#[tokio::test]
async fn test_naming_validation_over_http() {
    let server = test_support::http_server::get_global_server().await;
    // Reserved namespace names
        let reserved_names = ["system", "sys", "root", "kalamdb"];
        for name in reserved_names {
            let sql = format!("CREATE NAMESPACE {}", name);
            let response = server.execute_sql(&sql).await?;
            assert_eq!(
                response.status,
                ResponseStatus::Error,
                "Should reject reserved namespace name '{}'",
                name
            );
        }

        // Valid namespace names
        let valid_names = ["validns1", "validns2", "validns3"];
        for name in valid_names {
            let sql = format!("CREATE NAMESPACE {}", name);
            let response = server.execute_sql(&sql).await?;
            assert_eq!(
                response.status,
                ResponseStatus::Success,
                "Should accept valid namespace name '{}'",
                name
            );
            let _ = server
                .execute_sql(&format!("DROP NAMESPACE {}", name))
                .await;
        }

        // Reserved column names
        let response = server
            .execute_sql("CREATE NAMESPACE IF NOT EXISTS test_cols")
            .await?;
        assert_eq!(response.status, ResponseStatus::Success);

        let reserved_columns = ["_seq", "_deleted"];
        for col_name in reserved_columns {
            let table_name = format!("test_{}", col_name.replace('_', ""));
            let sql = format!(
                "CREATE TABLE test_cols.{} ({} TEXT PRIMARY KEY) WITH (TYPE = 'USER')",
                table_name, col_name
            );
            let response = server.execute_sql(&sql).await?;
            assert_eq!(
                response.status,
                ResponseStatus::Error,
                "Should reject reserved column name '{}'",
                col_name
            );
        }
        let _ = server.execute_sql("DROP NAMESPACE test_cols").await;

        // Valid column names
        let ns = format!("test_valid_cols_{}", std::process::id());
        let response = server
            .execute_sql(&format!("CREATE NAMESPACE IF NOT EXISTS {}", ns))
            .await?;
        assert_eq!(response.status, ResponseStatus::Success);

        let valid_columns = [("user_id", "users"), ("firstName", "profiles")];
        for (col_name, table_name) in valid_columns {
            let sql = format!(
                "CREATE TABLE {}.{} ({} TEXT PRIMARY KEY) WITH (TYPE = 'USER')",
                ns, table_name, col_name
            );
            let response = server.execute_sql(&sql).await?;
            assert_eq!(
                response.status,
                ResponseStatus::Success,
                "Should accept valid column name '{}'",
                col_name
            );
        }
        let _ = server.execute_sql(&format!("DROP NAMESPACE {}", ns)).await;

        // No auto id injection / basic CRUD
        let response = server
            .execute_sql("CREATE NAMESPACE IF NOT EXISTS test_no_id")
            .await?;
        assert_eq!(response.status, ResponseStatus::Success);

        let sql = "CREATE TABLE test_no_id.messages (message TEXT PRIMARY KEY, content TEXT) WITH (TYPE = 'USER')";
        let response = server.execute_sql(sql).await?;
        assert_eq!(response.status, ResponseStatus::Success);

        let insert_sql =
            "INSERT INTO test_no_id.messages (message, content) VALUES ('msg1', 'Hello')";
        let response = server.execute_sql(insert_sql).await?;
        assert_eq!(response.status, ResponseStatus::Success);

        let response = server
            .execute_sql("SELECT message, content FROM test_no_id.messages")
            .await?;
        assert_eq!(response.status, ResponseStatus::Success);

        let _ = server.execute_sql("DROP NAMESPACE test_no_id").await;

        // Users can use "id" as a column name
        let response = server
            .execute_sql("CREATE NAMESPACE IF NOT EXISTS test_user_id")
            .await?;
        assert_eq!(response.status, ResponseStatus::Success);
        let sql =
            "CREATE TABLE test_user_id.products (id TEXT PRIMARY KEY, name TEXT) WITH (TYPE = 'USER')";
        let response = server.execute_sql(sql).await?;
        assert_eq!(
            response.status,
            ResponseStatus::Success,
            "Should allow user-defined 'id' column"
        );
    let _ = server.execute_sql("DROP NAMESPACE test_user_id").await;
}
