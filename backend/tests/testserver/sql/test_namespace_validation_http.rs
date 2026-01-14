//! Namespace validation tests over the real HTTP SQL API.


use kalam_link::models::ResponseStatus;
use kalamdb_commons::UserName;
use super::test_support::http_server::HttpTestServer;
use tokio::time::Duration;

#[tokio::test]
#[ntest::timeout(60000)] // 60 seconds - namespace validation test
async fn test_namespace_validation_over_http() -> anyhow::Result<()> {
    let server = super::test_support::http_server::get_global_server().await;
    // CREATE TABLE should fail when namespace is missing.
    {
                let response = server
                    .execute_sql(
                        r#"CREATE TABLE missing_ns.audit_log (
                            id INT PRIMARY KEY,
                            action TEXT
                        )"#,
                    )
                    .await?;

                assert_eq!(response.status, ResponseStatus::Error);
                let error = response.error.expect("Expected an error payload");
                assert!(
                    error.message.contains("missing_ns"),
                    "Error should mention namespace: {:?}",
                    error
                );
                assert!(
                    error
                        .message
                        .contains("Create it first with CREATE NAMESPACE missing_ns"),
                    "Error should include recovery guidance: {:?}",
                    error
                );
            }

            // Once namespace exists, CREATE TABLE should work.
            {
                let create_sql = r#"CREATE TABLE audit.trail (
                    id INT PRIMARY KEY,
                    actor TEXT
                )"#;

                let initial = server.execute_sql(create_sql).await?;
                assert_eq!(initial.status, ResponseStatus::Error);

                let ns_response = server.execute_sql("CREATE NAMESPACE audit").await?;
                assert_eq!(ns_response.status, ResponseStatus::Success);

                let retry = server.execute_sql(create_sql).await?;
                assert_eq!(retry.status, ResponseStatus::Success);
            }

            // USER table namespace validation (real auth required).
            {
                let user = "user123";
                let password = "UserPass123!";
                let resp = server
                    .execute_sql(&format!(
                        "CREATE USER '{}' WITH PASSWORD '{}' ROLE 'user'",
                        user, password
                    ))
                    .await?;
                assert_eq!(resp.status, ResponseStatus::Success);

                let auth = HttpTestServer::basic_auth_header(&UserName::new(user), password);

                let sql = r#"CREATE TABLE workspace.notes (
                    id INT PRIMARY KEY,
                    content TEXT
                ) WITH (
                    TYPE = 'USER'
                )"#;

                let response = server.execute_sql_with_auth(sql, &auth).await?;
                assert_eq!(response.status, ResponseStatus::Error);

                let ns_resp = server.execute_sql("CREATE NAMESPACE workspace").await?;
                assert_eq!(ns_resp.status, ResponseStatus::Success);

                let retry = server.execute_sql_with_auth(sql, &auth).await?;
                assert_eq!(retry.status, ResponseStatus::Success);
            }

            // Shared table namespace validation.
            {
                let response = server
                    .execute_sql(
                        r#"CREATE TABLE ops.config (
                            setting TEXT,
                            value TEXT
                        ) WITH (
                            TYPE = 'SHARED'
                        )"#,
                    )
                    .await?;
                assert_eq!(response.status, ResponseStatus::Error);

                let ns_resp = server.execute_sql("CREATE NAMESPACE ops").await?;
                assert_eq!(ns_resp.status, ResponseStatus::Success);

                let retry = server
                    .execute_sql(
                        r#"CREATE TABLE ops.config (
                            setting TEXT PRIMARY KEY,
                            value TEXT
                        ) WITH (
                            TYPE = 'SHARED'
                        )"#,
                    )
                    .await?;
                assert_eq!(retry.status, ResponseStatus::Success);
            }

            // Stream table namespace validation.
            {
                let response = server
                    .execute_sql(
                        r#"CREATE TABLE telemetry.events (
                            event_id TEXT,
                            payload TEXT
                        ) WITH (
                            TYPE = 'STREAM',
                            TTL_SECONDS = 60
                        )"#,
                    )
                    .await?;
                assert_eq!(response.status, ResponseStatus::Error);

                let ns_resp = server.execute_sql("CREATE NAMESPACE telemetry").await?;
                assert_eq!(ns_resp.status, ResponseStatus::Success);

                let retry = server
                    .execute_sql(
                        r#"CREATE TABLE telemetry.events (
                            event_id TEXT,
                            payload TEXT
                        ) WITH (
                            TYPE = 'STREAM',
                            TTL_SECONDS = 60
                        )"#,
                    )
                    .await?;
                assert_eq!(retry.status, ResponseStatus::Success);
            }

            // Race: CREATE TABLE vs CREATE NAMESPACE concurrently.
            {
                let table_sql = r#"CREATE TABLE race_ns.logs (
                    id INT PRIMARY KEY,
                    message TEXT
                )"#;

                let create_table = server.execute_sql(table_sql);
                let create_namespace = async {
                    tokio::time::sleep(Duration::from_millis(10)).await;
                    server.execute_sql("CREATE NAMESPACE race_ns").await
                };

                let (table_result, namespace_result) = tokio::join!(create_table, create_namespace);
                let table_result = table_result?;
                let namespace_result = namespace_result?;

                assert_eq!(namespace_result.status, ResponseStatus::Success);
                assert!(
                    matches!(table_result.status, ResponseStatus::Success | ResponseStatus::Error),
                    "Unexpected status: {:?}",
                    table_result.status
                );

                // If CREATE TABLE lost the race, it should succeed if retried.
                if table_result.status == ResponseStatus::Error {
                    let retry = server.execute_sql(table_sql).await?;
                    assert_eq!(retry.status, ResponseStatus::Success);
                }
            }
    Ok(())
}
